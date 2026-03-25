use anyhow::Result;
use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

pub(crate) mod hash;
pub(crate) mod worker;

pub struct DownloadProgress {
    pub fraction: f32,
    pub label: String,
}

pub fn download_attachments<F>(
    results_dir: &Path,
    links: Vec<String>,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    if links.is_empty() {
        on_progress(DownloadProgress {
            fraction: 1.0,
            label: "No attachments to download.".to_owned(),
        });
        return Ok(());
    }

    let mut seen = HashSet::new();
    let mut unique_links = Vec::new();
    let mut duplicates_ignored_url = 0usize;

    for url in links {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            continue;
        }
        let dedupe_key = hash::canonical_attachment_key(trimmed);
        if seen.insert(dedupe_key.clone()) {
            unique_links.push((trimmed.to_owned(), dedupe_key));
        } else {
            duplicates_ignored_url += 1;
        }
    }

    if unique_links.is_empty() {
        on_progress(DownloadProgress {
            fraction: 1.0,
            label: "No attachments to download.".to_owned(),
        });
        return Ok(());
    }

    let total = unique_links.len();
    let downloaded = Arc::new(Mutex::new(0usize));
    let skipped = Arc::new(Mutex::new(0usize));
    let duplicate_content = Arc::new(Mutex::new(0usize));
    let failed = Arc::new(Mutex::new(0usize));

    let categories = [
        "unknowns", "audios", "docs", "imgs", "txts", "codes", "data", "exes", "vids", "zips",
    ];
    for category in categories {
        fs::create_dir_all(results_dir.join(category))?;
    }

    let temp_dir = results_dir.join(".tmp_downloads");
    fs::create_dir_all(&temp_dir)?;

    let hash_index_path = results_dir.join("attachment_hash_index.json");
    let hash_index = Arc::new(Mutex::new(hash::load_hash_index(&hash_index_path)?));
    let in_flight_hashes = Arc::new(Mutex::new(HashSet::<String>::new()));

    let mut tasks = Vec::new();
    for (i, (url, dedupe_key)) in unique_links.into_iter().enumerate() {
        let category = guess_category(&dedupe_key);
        let base_name = attachment_basename(&dedupe_key);
        let safe_name = base_name.replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-',
            "_",
        );
        let output_name = format!("attachment_{:06}_{}", i, safe_name);
        let output_path = results_dir.join(category).join(&output_name);
        let temp_path = temp_dir.join(format!("attachment_{:06}.part", i));
        tasks.push(worker::DownloadTask {
            url,
            output_path,
            temp_path,
        });
    }

    let (tx, rx) = mpsc::channel();
    let num_workers = 4;
    let queue = Arc::new(Mutex::new(tasks));

    for _ in 0..num_workers {
        let queue = queue.clone();
        let tx = tx.clone();
        let downloaded = downloaded.clone();
        let skipped = skipped.clone();
        let duplicate_content = duplicate_content.clone();
        let failed = failed.clone();
        let hash_index = hash_index.clone();
        let in_flight_hashes = in_flight_hashes.clone();

        thread::spawn(move || {
            loop {
                let task = {
                    let mut q = queue.lock().unwrap();
                    q.pop()
                };
                let Some(task) = task else { break };

                if task.output_path.exists()
                    && task.output_path.metadata().map(|m| m.len()).unwrap_or(0) > 0
                {
                    if let Ok(hash) = hash::hash_file_sha256(&task.output_path) {
                        let path_text = task.output_path.to_string_lossy().to_string();
                        let mut index = hash_index.lock().unwrap();
                        index.hashes.entry(hash).or_insert(path_text);
                    }
                    *skipped.lock().unwrap() += 1;
                    let _ = tx.send(());
                    continue;
                }

                let mut handled = false;
                let mut saved_new_file = false;
                for _attempt in 1..=3 {
                    let _ = fs::remove_file(&task.temp_path);
                    let content_hash =
                        match worker::download_to_temp_and_hash(&task.url, &task.temp_path) {
                            Ok(hash) => hash,
                            Err(_) => {
                                thread::sleep(Duration::from_secs(1));
                                continue;
                            }
                        };

                    let mut claimed_hash = {
                        let mut in_flight = in_flight_hashes.lock().unwrap();
                        if in_flight.contains(&content_hash) {
                            false
                        } else {
                            in_flight.insert(content_hash.clone());
                            true
                        }
                    };

                    if !claimed_hash {
                        for _ in 0..40 {
                            thread::sleep(Duration::from_millis(150));
                            let exists_now = {
                                let index = hash_index.lock().unwrap();
                                index
                                    .hashes
                                    .get(&content_hash)
                                    .map(|p| Path::new(p).exists())
                                    .unwrap_or(false)
                            };
                            if exists_now {
                                let _ = fs::remove_file(&task.temp_path);
                                *duplicate_content.lock().unwrap() += 1;
                                handled = true;
                                break;
                            }
                            let can_claim_now = {
                                let mut in_flight = in_flight_hashes.lock().unwrap();
                                if in_flight.contains(&content_hash) {
                                    false
                                } else {
                                    in_flight.insert(content_hash.clone());
                                    true
                                }
                            };
                            if can_claim_now {
                                claimed_hash = true;
                                break;
                            }
                        }
                        if handled {
                            break;
                        }
                        if !claimed_hash {
                            let _ = fs::remove_file(&task.temp_path);
                            thread::sleep(Duration::from_secs(1));
                            continue;
                        }
                    }

                    let output_path_text = task.output_path.to_string_lossy().to_string();
                    let is_content_duplicate = {
                        let index = hash_index.lock().unwrap();
                        matches!(index.hashes.get(&content_hash), Some(existing_path) if Path::new(existing_path).exists())
                    };

                    if is_content_duplicate {
                        let _ = fs::remove_file(&task.temp_path);
                        let mut in_flight = in_flight_hashes.lock().unwrap();
                        in_flight.remove(&content_hash);
                        *duplicate_content.lock().unwrap() += 1;
                        handled = true;
                        break;
                    }

                    match worker::finalize_temp_file(&task.temp_path, &task.output_path) {
                        Ok(()) => {
                            {
                                let mut index = hash_index.lock().unwrap();
                                index.hashes.insert(content_hash.clone(), output_path_text);
                            }
                            {
                                let mut in_flight = in_flight_hashes.lock().unwrap();
                                in_flight.remove(&content_hash);
                            }
                            handled = true;
                            saved_new_file = true;
                            break;
                        }
                        Err(_) => {
                            let _ = fs::remove_file(&task.temp_path);
                            let mut in_flight = in_flight_hashes.lock().unwrap();
                            in_flight.remove(&content_hash);
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                }

                if saved_new_file {
                    *downloaded.lock().unwrap() += 1;
                } else if !handled {
                    *failed.lock().unwrap() += 1;
                }
                let _ = tx.send(());
            }
        });
    }

    drop(tx);
    let mut completed = 0;
    while let Ok(()) = rx.recv() {
        completed += 1;
        let d = *downloaded.lock().unwrap();
        let s = *skipped.lock().unwrap();
        let dc = *duplicate_content.lock().unwrap();
        let f = *failed.lock().unwrap();
        on_progress(DownloadProgress {
            fraction: completed as f32 / total as f32,
            label: format!(
                "Downloading: {}/{} ({} saved, {} existing, {} dup-content, {} failed, {} dup-url)",
                completed, total, d, s, dc, f, duplicates_ignored_url
            ),
        });
    }

    {
        let index = hash_index.lock().unwrap();
        hash::save_hash_index(&hash_index_path, &index)?;
    }

    let d = *downloaded.lock().unwrap();
    let s = *skipped.lock().unwrap();
    let dc = *duplicate_content.lock().unwrap();
    let f = *failed.lock().unwrap();
    on_progress(DownloadProgress {
        fraction: 1.0,
        label: format!(
            "Download finished. {} saved, {} existing, {} dup-content, {} failed, {} dup-url.",
            d, s, dc, f, duplicates_ignored_url
        ),
    });

    Ok(())
}

fn attachment_basename(url_or_path: &str) -> &str {
    let trimmed = url_or_path.trim_end_matches('/');
    let segment = trimmed.rsplit('/').next().unwrap_or("attachment");
    if segment.is_empty() {
        "attachment"
    } else {
        segment
    }
}

fn guess_category(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".mp3") || lower.contains(".wav") || lower.contains(".m4a") {
        "audios"
    } else if lower.contains(".doc") || lower.contains(".pdf") {
        "docs"
    } else if lower.contains(".jpg")
        || lower.contains(".jpeg")
        || lower.contains(".png")
        || lower.contains(".gif")
        || lower.contains(".webp")
    {
        "imgs"
    } else if lower.contains(".txt") {
        "txts"
    } else if lower.contains(".py")
        || lower.contains(".js")
        || lower.contains(".html")
        || lower.contains(".css")
        || lower.contains(".json")
    {
        "codes"
    } else if lower.contains(".exe") || lower.contains(".msi") {
        "exes"
    } else if lower.contains(".mp4")
        || lower.contains(".mov")
        || lower.contains(".webm")
        || lower.contains(".mkv")
    {
        "vids"
    } else if lower.contains(".zip")
        || lower.contains(".rar")
        || lower.contains(".7z")
        || lower.contains(".tar")
        || lower.contains(".gz")
    {
        "zips"
    } else {
        "unknowns"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::PathBuf,
        sync::atomic::{AtomicBool, Ordering},
        time::UNIX_EPOCH,
    };

    struct TestServer {
        base_url: String,
        stop: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn start() -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let addr = listener.local_addr()?;
            let stop = Arc::new(AtomicBool::new(false));
            let stop_for_thread = stop.clone();
            let handle = thread::spawn(move || {
                while !stop_for_thread.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => handle_client(stream),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
            });
            Ok(Self {
                base_url: format!("http://{addr}"),
                stop,
                handle: Some(handle),
            })
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect(
                self.base_url
                    .strip_prefix("http://")
                    .unwrap_or("127.0.0.1:1"),
            );
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn handle_client(mut stream: TcpStream) {
        let mut request = Vec::new();
        let mut buf = [0_u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    request.extend_from_slice(&buf[..n]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                    if request.len() > 1024 * 32 {
                        break;
                    }
                }
                Err(_) => return,
            }
        }
        let request_text = String::from_utf8_lossy(&request);
        let first_line = request_text.lines().next().unwrap_or("");
        let raw_path = first_line.split_whitespace().nth(1).unwrap_or("/");
        let path = raw_path.split('?').next().unwrap_or(raw_path);
        let (status, body): (&str, &[u8]) = match path {
            "/assets/one.png" => ("200 OK", b"SAME_CONTENT"),
            "/dmA/mirror.mp4" => ("200 OK", b"SAME_CONTENT"),
            "/dmB/unique.txt" => ("200 OK", b"UNIQUE_CONTENT"),
            _ => ("404 Not Found", b"NOT_FOUND"),
        };
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    }

    fn make_temp_results_dir() -> Result<PathBuf> {
        let ts = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "discord-data-analyzer-test-{}-{}",
            std::process::id(),
            ts
        ));
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn count_saved_attachments(results_dir: &Path) -> Result<usize> {
        let mut count = 0usize;
        for entry in fs::read_dir(results_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".tmp_downloads" {
                continue;
            }
            for file in fs::read_dir(&path)? {
                let fp = file?.path();
                if fp.is_file()
                    && fp
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .starts_with("attachment_")
                {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    #[test]
    fn dedupes_url_and_content_across_runs() -> Result<()> {
        let server = TestServer::start()?;
        let results_dir = make_temp_results_dir()?;
        let links = vec![
            format!("{}/assets/one.png?token=1", server.base_url),
            format!("{}/assets/one.png?token=2", server.base_url),
            format!("{}/dmA/mirror.mp4", server.base_url),
            format!("{}/dmB/unique.txt", server.base_url),
        ];

        let first_final_label = Arc::new(Mutex::new(String::new()));
        {
            let label_ref = first_final_label.clone();
            download_attachments(&results_dir, links.clone(), move |progress| {
                if let Ok(mut label) = label_ref.lock() {
                    *label = progress.label;
                }
            })?;
        }
        let first_label = first_final_label.lock().unwrap().clone();
        assert!(first_label.contains("2 saved"));
        assert!(first_label.contains("1 dup-content"));
        assert!(first_label.contains("1 dup-url"));
        assert_eq!(count_saved_attachments(&results_dir)?, 2);

        let index_path = results_dir.join("attachment_hash_index.json");
        assert!(index_path.is_file());

        let second_final_label = Arc::new(Mutex::new(String::new()));
        {
            let label_ref = second_final_label.clone();
            download_attachments(&results_dir, links, move |progress| {
                if let Ok(mut label) = label_ref.lock() {
                    *label = progress.label;
                }
            })?;
        }
        let second_label = second_final_label.lock().unwrap().clone();
        assert!(second_label.contains("0 saved"));
        assert!(second_label.contains("2 existing"));
        assert!(second_label.contains("1 dup-content"));
        assert!(second_label.contains("1 dup-url"));

        let _ = fs::remove_dir_all(&results_dir);
        Ok(())
    }
}
