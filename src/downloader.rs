use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use anyhow::Result;

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

    let total = links.len();
    let downloaded = Arc::new(Mutex::new(0usize));
    let skipped = Arc::new(Mutex::new(0usize));
    let failed = Arc::new(Mutex::new(0usize));

    let categories = ["unknowns", "audios", "docs", "imgs", "txts", "codes", "data", "exes", "vids", "zips"];
    for category in categories {
        fs::create_dir_all(results_dir.join(category))?;
    }

    // Assign category and safe name
    let mut tasks = Vec::new();
    for (i, url) in links.into_iter().enumerate() {
        let category = guess_category(&url);
        let base_name = url.split('/').last().unwrap_or("attachment").split('?').next().unwrap_or("attachment");
        let safe_name = base_name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-', "_");
        let output_name = format!("attachment_{}_{}", i, safe_name);
        let output_path = results_dir.join(category).join(&output_name);
        tasks.push((url, output_path));
    }

    let (tx, rx) = mpsc::channel();
    let num_workers = 4;
    let queue = Arc::new(Mutex::new(tasks));

    for _ in 0..num_workers {
        let queue = queue.clone();
        let tx = tx.clone();
        let downloaded = downloaded.clone();
        let skipped = skipped.clone();
        let failed = failed.clone();

        thread::spawn(move || {
            loop {
                let task = {
                    let mut q = queue.lock().unwrap();
                    q.pop()
                };
                let Some((url, output_path)) = task else { break };

                if output_path.exists() && output_path.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
                    *skipped.lock().unwrap() += 1;
                    let _ = tx.send(());
                    continue;
                }

                let mut success = false;
                for _attempt in 1..=3 {
                    if let Ok(resp) = ureq::get(&url).header("User-Agent", "Mozilla/5.0").call() {
                        let mut reader = resp.into_body().into_reader();
                        if let Ok(mut file) = File::create(&output_path) {
                            let mut buffer = [0; 65536];
                            let mut file_success = true;
                            loop {
                                match reader.read(&mut buffer) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        if file.write_all(&buffer[..n]).is_err() {
                                            file_success = false;
                                            break;
                                        }
                                    }
                                    Err(_) => {
                                        file_success = false;
                                        break;
                                    }
                                }
                            }
                            if file_success {
                                success = true;
                                break;
                            }
                        }
                    }
                    thread::sleep(Duration::from_secs(1));
                }

                if success {
                    *downloaded.lock().unwrap() += 1;
                } else {
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
        let f = *failed.lock().unwrap();
        on_progress(DownloadProgress {
            fraction: completed as f32 / total as f32,
            label: format!("Downloading: {}/{} ({} ok, {} skipped, {} failed)", completed, total, d, s, f),
        });
    }

    let d = *downloaded.lock().unwrap();
    let s = *skipped.lock().unwrap();
    let f = *failed.lock().unwrap();
    on_progress(DownloadProgress {
        fraction: 1.0,
        label: format!("Download finished. {} ok, {} skipped, {} failed.", d, s, f),
    });

    Ok(())
}

fn guess_category(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".mp3") || lower.contains(".wav") || lower.contains(".m4a") {
        "audios"
    } else if lower.contains(".doc") || lower.contains(".pdf") {
        "docs"
    } else if lower.contains(".jpg") || lower.contains(".jpeg") || lower.contains(".png") || lower.contains(".gif") || lower.contains(".webp") {
        "imgs"
    } else if lower.contains(".txt") {
        "txts"
    } else if lower.contains(".py") || lower.contains(".js") || lower.contains(".html") || lower.contains(".css") || lower.contains(".json") {
        "codes"
    } else if lower.contains(".exe") || lower.contains(".msi") {
        "exes"
    } else if lower.contains(".mp4") || lower.contains(".mov") || lower.contains(".webm") || lower.contains(".mkv") {
        "vids"
    } else if lower.contains(".zip") || lower.contains(".rar") || lower.contains(".7z") || lower.contains(".tar") || lower.contains(".gz") {
        "zips"
    } else {
        "unknowns"
    }
}
