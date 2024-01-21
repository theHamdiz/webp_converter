
use image::io::Reader as ImageReader;
use image::{ImageFormat};
use std::{fs, io};
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use log::{info, error, warn, Level};
use env_logger;
use env_logger::Builder;
use std::env;
use clap::Parser;
use colored::Colorize;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "info");

    let _ = Builder::new()
        .format(|buf, record| {
            let level = record.level();
            let message = match level {
                Level::Error => record.args().to_string().bright_red(),
                Level::Warn => record.args().to_string().bright_yellow(),
                Level::Info => record.args().to_string().bright_cyan().bold(),
                Level::Debug => record.args().to_string().bright_purple(),
                Level::Trace => record.args().to_string().normal(),
            };

            writeln!(
                buf,
                "{} - {}",
                level,
                message
            )
        })
        .init();

    let args = helpers::Args::parse();
    let directory_path = args.path.unwrap_or_else(|| {
        info!("{}", "Please provide a directory path:".purple().bold());
        io::stdout().flush().unwrap(); // Make sure the prompt is displayed immediately
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string() // Remove the newline character at the end
    });

    let compress = args.compress;
    let recursive = args.recursive.unwrap_or(false);


    let path = helpers::process_path_for_os(directory_path);
    let path_buff = PathBuf::from(path);

    if ! path_buff.exists() {
        let msg = "Path does not exist, terminating....".red().underline();
        error!("{}", msg); return;
    }

    let msg = format!("Path: {}", path_buff.to_string_lossy()).green().underline();
    info!("{}", msg);

    if path_buff.is_dir(){
        info!("{}", "Directory Detected Working on it...".blue());
        converter::convert_images_to_webp(path_buff, recursive, compress).await;
    }else{
        info!("{}", "Single Image File Detected...".blue());
        let _ = converter::convert_single_photo(path_buff, compress).await;
    }
}

pub(crate) mod helpers{
    use clap::Parser;
    use walkdir::DirEntry;
    use super::*;


    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None)]
    pub(crate) struct Args{
        #[arg(short = 'p', long = "path")]
        pub(crate) path: Option<String>,
        #[arg(short = 'c', long = "compress")]
        pub(crate) compress: Option<bool>,
        #[arg(short = 'r', long = "recursive")]
        pub(crate) recursive: Option<bool>,
    }

    pub(crate) enum Actions{
        Convert,
        Copy,
        Nothing
    }
    pub(crate) fn which_action(path: DirEntry) -> Actions {
        // Check if the file is an image and should be converted or copied.
        let p = path.path().to_string_lossy().to_string().replace('"', "");
        let path = PathBuf::from(&p);
        match path.extension().and_then(|e| e.to_str()) {
            Some("jpg") | Some("jpeg") | Some("png") | Some("tiff") | Some("bmp") | Some("avif") | Some("gif") => Actions::Convert,
            Some("webp") => Actions::Copy,
            _ => Actions::Nothing,
        }
    }



    pub(crate) fn process_path_for_os<S: Into<String>>(path: S) -> String {
        let mut path = path.into();
        info!("{}", format!("Path before modifications: {}", path).green().bold());
        #[cfg(windows)]
        {
            // For Windows, if the path contains spaces and is not already quoted, quote it.
            if path.contains(' ') && !path.starts_with('"') && !path.ends_with('"') {
                warn!("{}", format!("Path contains spaces, wrapping in quotes: {}", path).yellow().bold());
                // return format!("\"{}\"", path);
            }

            path = path.replace("/", "\\"); // Convert Unix-style slashes to Windows-style.
            path = path.replace('\\', "\\"); // Convert spaces to windows separators.

            path
        }
        #[cfg(not(windows))]
        {
            path = path.replace('\\', "/"); // Convert to unix style.
            // For Unix-like systems, ensure the path is escaped properly.
            // This simplistic approach handles spaces; adapt as needed for other special characters.
            path.replace(" ", "\\ ")
        }
    }
}

pub(crate) mod wio{
    use super::*;
    pub(crate) async fn copy_image_to_output_folder(p0: &Path) -> Result<(), io::Error> {
        let filename = p0.file_name().unwrap();

        let copy_path = get_or_create_output_directory(p0)
            .join(filename);
        fs::copy(p0, copy_path.clone())?;


        if let Some(last_component) = get_or_create_output_directory(p0).components().last() {
            match last_component {
                std::path::Component::Normal(name) => {
                    #[cfg(windows)]
                    info!("\n{}\n", format!("Copying: {:?} to {:?}\\{:?}", p0.file_name().unwrap(), name, copy_path.file_name().unwrap()).bright_blue().bold());
                    #[cfg(not(windows))]
                    info!("{}", format!("Copying: {:?} to {:?}/{:?}", p0.file_name().unwrap(), name, copy_path.file_name().unwrap()).bright_blue().bold());
                },
                _ => println!("The last component is not a normal directory or file name."),
            }
        }


        Ok(())
    }

    pub(crate) fn get_or_create_output_directory(path: &Path) -> PathBuf {
        // Create the "webp_converter" directory inside the original image's directory
        let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
        let webp_dir = parent_dir.join("webp_converter_output");
        if webp_dir.exists(){
            webp_dir
        }else{
            fs::create_dir_all(&webp_dir).unwrap();
            webp_dir
        }
    }

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    pub(crate) fn make_file_writable<P: AsRef<Path>>(path: P) -> io::Result<()> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)?;
        let mut perms = metadata.permissions();

        #[cfg(windows)]
        {
            perms.set_readonly(false);
        }

        #[cfg(unix)]
        {
            let mode = perms.mode();
            let new_mode = mode | 0o200;
            perms.set_mode(new_mode);
        }

        fs::set_permissions(path, perms)?;
        Ok(())
    }

}

pub(crate) mod converter {
    use std::sync::Arc;
    use image::codecs::webp::{WebPEncoder, WebPQuality};
    use image::{ImageError, RgbaImage};

    use super::*;

    pub(crate) async fn convert_images_to_webp<P: Into<PathBuf>>(path: P, recursive: bool, compress: Option<bool>) {
        let path = path.into();
        let cpu_cores = num_cpus::get();
        let max_concurrency = std::cmp::max(1, cpu_cores - 1); // Reserve one core for the system
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        let mut tasks = vec![];

        // Configure WalkDir based on the `recursive` flag
        let walker = WalkDir::new(&path);
        let walker = if recursive { walker } else { walker.min_depth(1).max_depth(1) }.into_iter();

        for entry in walker.filter_map(|e| e.ok()).filter(|e| e.path().is_file()) {
            match helpers::which_action(entry.clone()) {
                helpers::Actions::Convert => {
                    let sem_clone = semaphore.clone();
                    let entry_path = entry.into_path();

                    let task = tokio::spawn(async move {
                        let _permit = sem_clone.acquire().await.expect("Failed to acquire semaphore permit");
                        let _ = convert_single_photo(&entry_path, compress).await;
                    });

                    tasks.push(task);
                },
                helpers::Actions::Copy => {
                    let sem_clone = semaphore.clone();
                    let entry_path = entry.into_path();

                    let task = tokio::spawn(async move {
                        let _permit = sem_clone.acquire().await.expect("Failed to acquire semaphore permit");
                        wio::copy_image_to_output_folder(&entry_path).await.expect("Failed to copy image");
                    });

                    tasks.push(task);
                },
                helpers::Actions::Nothing => warn!("\n{}\n", format!("Not a valid image file: {:?}", entry.path()).yellow().bold()),
            }
        }

        // Await all tasks to complete
        for task in tasks {
            task.await.expect("Task failed to complete");
        }
    }

    pub(crate) async fn convert_single_photo<P: Into<PathBuf>>(path: P, compress: Option<bool>) -> Result<(), ImageError> {
        let compress_it = compress.unwrap_or(true);
        let path = path.into();
        let mut webp_dir = wio::get_or_create_output_directory(&path);

        if let Some(filename) = path.with_extension("webp").file_name() {
            webp_dir = webp_dir.join(filename);
        } else {
            webp_dir = webp_dir.join(path.file_name().unwrap());
        }

        wio::make_file_writable(&path).unwrap_or_else(|_| {});
        let img = image::open(path.clone())?;
        let f = fs::File::create(&webp_dir)?;

        match compress_it {
            true => {
                let rgba_img: RgbaImage = img.to_rgba8();

                let encoder = WebPEncoder::new_lossless(&f);
                let (width, height) = rgba_img.dimensions();

                // let quality = WebPQuality::default();
                // let encoder = image::codecs::webp::WebPEncoder::new_with_quality(f, quality);
                // encoder.encode(&rgba_img, width, height, image::ColorType::Rgba8)?;
                //
                // Ok(())

                match encoder.encode(&rgba_img, width, height, img.color()) {
                    Ok(i) => { Ok::<(), ImageError>(i) },
                    Err(e) => {
                        error!("\n{}\n", format!("Failed to encode image: {:?}", e).bright_red().bold());
                        img.save_with_format(webp_dir, ImageFormat::WebP)?;
                        Ok(())
                    }
                }
            },
            false => {
                img.save_with_format(webp_dir, ImageFormat::WebP)?;
                Ok(())
            },
        }.expect("Failed to save image");
        info!("\n{}\n", format!("Converted: {:?}", path).bright_green().bold());
        Ok(())
    }

}





