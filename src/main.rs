use clap::Parser;
use colored::Colorize;
use env_logger;
use log::{error, info, warn};
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};
use tokio::sync::Semaphore;
use tokio::task::spawn_blocking;
use walkdir::WalkDir;

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let args = helpers::Args::parse();
    let directory_path = args.path.unwrap_or_else(|| {
        info!("{}", "Please provide a directory path:".purple().bold());
        io::stdout().flush().unwrap(); // Make sure the prompt is displayed immediately
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string() // Remove the newline character at the end
    });

    let recursive = args.recursive.unwrap_or(false);
    let quality = args.quality.unwrap_or(75.0);

    let compression_factor = args.compression_factor.unwrap_or(2.0);

    let lossless = if compression_factor != 0.0 || quality < 100.0 {
        0
    } else {
        match args.lossless.unwrap_or(true) {
            true => 1,
            false => 0,
        }
    };

    let should_resize = args.resize.unwrap_or(false);

    let path = helpers::process_path_for_os(directory_path);
    let path_buff = PathBuf::from(path);
    let noise_ratio = args.psnr.unwrap_or(40.0);

    if !path_buff.exists() {
        let msg = "Path does not exist, terminating....".red().underline();
        error!("{}", msg);
        return;
    }

    let msg = format!("Path: {}", path_buff.to_string_lossy())
        .green()
        .underline();
    info!("{}", msg);

    if path_buff.is_dir() {
        info!(
            "{}",
            "Directory Detected Working on it...".bright_cyan().bold()
        );
        converter::convert_images_to_webp(
            path_buff,
            recursive,
            quality,
            lossless,
            compression_factor,
            should_resize,
            noise_ratio,
        )
        .await;
    } else {
        info!("{}", "Single Image File Detected...".bright_blue().bold());
        let _ = converter::convert_single_photo(
            path_buff,
            quality,
            lossless,
            compression_factor,
            should_resize,
            noise_ratio,
        )
        .await;
    }
}

pub(crate) mod types {
    use colored::Colorize;
    use std::fmt::Display;
    use std::io;
    use std::path::PathBuf;
    use tokio::task::JoinError;
    use webp::WebPMemory;

    #[derive(Debug, Clone)]
    pub(crate) struct WebpConverterError {
        pub(crate) message: String,
    }

    impl From<image::ImageError> for WebpConverterError {
        fn from(error: image::ImageError) -> Self {
            WebpConverterError {
                message: format!("Image Error: {:?}", error),
            }
        }
    }

    impl From<io::Error> for WebpConverterError {
        fn from(error: io::Error) -> Self {
            WebpConverterError {
                message: format!("IO Error: {:?}", error),
            }
        }
    }

    impl From<webp::WebPEncodingError> for WebpConverterError {
        fn from(error: webp::WebPEncodingError) -> Self {
            WebpConverterError {
                message: format!("WebP Encoding Error: {:?}", error),
            }
        }
    }

    impl Display for WebpConverterError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", format!("Error: {}", self.message).red().bold())
        }
    }

    impl From<Result<PathBuf, WebpConverterError>> for WebpConverterError {
        fn from(error: Result<PathBuf, WebpConverterError>) -> Self {
            match error {
                Ok(_) => WebpConverterError {
                    message: "Unknown Error".to_string(),
                },
                Err(e) => e,
            }
        }
    }

    impl From<Result<WebPMemory, WebpConverterError>> for WebpConverterError {
        fn from(error: Result<WebPMemory, WebpConverterError>) -> Self {
            match error {
                Ok(_) => WebpConverterError {
                    message: "Unknown Error".to_string(),
                },
                Err(e) => e,
            }
        }
    }

    impl From<JoinError> for WebpConverterError {
        fn from(error: JoinError) -> Self {
            WebpConverterError {
                message: format!("Join Error: {:?}", error),
            }
        }
    }
}

pub(crate) mod helpers {
    use super::*;
    use clap::Parser;
    use walkdir::DirEntry;

    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None)]
    pub(crate) struct Args {
        #[arg(short = 'p', long = "PATH")]
        pub(crate) path: Option<String>,
        #[arg(short = 'r', long = "RECURSIVE")]
        pub(crate) recursive: Option<bool>,
        #[arg(short = 'q', long = "QUALITY", default_value = "75")]
        pub(crate) quality: Option<f32>,
        #[arg(short = 'l', long = "LOSSLESS", default_value = "true")]
        pub(crate) lossless: Option<bool>,
        #[arg(short = 'c', long = "COMPRESSIONFACTOR", default_value = "0.0")]
        pub(crate) compression_factor: Option<f32>,
        #[arg(short = 's', long = "RESIZE")]
        pub(crate) resize: Option<bool>,
        #[arg(short = 'n', long = "NOISERATIO")]
        pub(crate) psnr: Option<f32>,
    }

    pub(crate) enum Actions {
        Convert,
        Copy,
        Nothing,
    }
    pub(crate) fn which_action(path: DirEntry) -> Actions {
        // Check if the file is an image and should be converted or copied.
        let p = path.path().to_string_lossy().to_string().replace('"', "");
        let path = PathBuf::from(&p);
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
        {
            Some(extension)
                if [
                    "jpg", "jpeg", "png", "tiff", "tif", "bmp", "avif", "gif", "jfif",
                ]
                .contains(&extension.as_str()) =>
            {
                Actions::Convert
            }
            Some(extension) if extension == "webp" => Actions::Copy,
            _ => Actions::Nothing,
        }
    }

    pub(crate) fn process_path_for_os<S: Into<String>>(path: S) -> String {
        let mut path = path.into();
        info!(
            "{}",
            format!("Path before modifications: {}", path)
                .green()
                .bold()
        );
        #[cfg(windows)]
        {
            // For Windows, if the path contains spaces and is not already quoted, quote it.
            if path.contains(' ') && !path.starts_with('"') && !path.ends_with('"') {
                warn!(
                    "{}",
                    format!("Path contains spaces, wrapping in quotes: {}", path)
                        .yellow()
                        .bold()
                );
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

pub(crate) mod wio {
    use super::*;
    pub(crate) async fn copy_image_to_output_folder(p0: &Path) -> Result<(), io::Error> {
        let filename = p0.file_name().unwrap();

        let copy_path = get_or_create_output_directory(p0).join(filename);
        fs::copy(p0, copy_path.clone())?;

        if let Some(last_component) = get_or_create_output_directory(p0).components().last() {
            match last_component {
                std::path::Component::Normal(name) => {
                    #[cfg(windows)]
                    info!(
                        "\n{}\n",
                        format!(
                            "Copying: {:?} to {:?}\\{:?}",
                            p0.file_name().unwrap(),
                            name,
                            copy_path.file_name().unwrap()
                        )
                        .bright_blue()
                        .bold()
                    );
                    #[cfg(not(windows))]
                    info!(
                        "{}",
                        format!(
                            "Copying: {:?} to {:?}/{:?}",
                            p0.file_name().unwrap(),
                            name,
                            copy_path.file_name().unwrap()
                        )
                        .bright_blue()
                        .bold()
                    );
                }
                _ => println!("The last component is not a normal directory or file name."),
            }
        }

        Ok(())
    }

    pub(crate) fn get_or_create_output_directory(path: &Path) -> PathBuf {
        // Create the "webp_converter" directory inside the original image's directory
        let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
        let webp_dir = parent_dir.join("webp_converter_output");
        if webp_dir.exists() {
            webp_dir
        } else {
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

    pub(crate) fn cleanup(workspace_path: PathBuf) -> io::Result<()> {
        let output_dir = get_or_create_output_directory(&workspace_path);
        if workspace_path.exists() {
            // check for empty or zero bytes files
            // delete them from the filesystem.
            for entry in fs::read_dir(output_dir)? {
                let entry = entry?;
                let file_size = entry.metadata()?.len();
                if file_size == 0 {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }
}

pub(crate) mod converter {
    use crate::types::WebpConverterError;
    use image::imageops::FilterType;
    use image::{DynamicImage, GenericImageView, RgbaImage};
    use std::io::ErrorKind;
    use std::sync::Arc;
    use tokio::io::{AsyncWriteExt, BufWriter};
    use webp::WebPMemory;

    use super::*;

    // Function to decide on using resized_img or img
    pub(crate) async fn decide_and_encode(
        img: DynamicImage,
        resized_img: DynamicImage,
        quality: f32,
        lossless: i32,
        noise_ratio: f32,
        target_size: i32,
    ) -> Result<Vec<u8>, WebpConverterError> {
        // Encode both images to WebP format in memory to compare file sizes
        let original_encoded =
            encode_webp(quality, lossless, noise_ratio, target_size, img).await?;
        let resized_encoded =
            encode_webp(quality, lossless, noise_ratio, target_size, resized_img).await?;
        // Use the smaller one, or the original if sizes are equal
        // This is a simplistic approach; you might choose based on other criteria
        if resized_encoded.len() < original_encoded.len() {
            Ok(resized_encoded)
        } else {
            Ok(original_encoded)
        }
    }

    pub(crate) async fn convert_images_to_webp<P: Into<PathBuf>>(
        path: P,
        recursive: bool,
        quality: f32,
        lossless: i32,
        compression_factor: f32,
        should_resize: bool,
        noise_ratio: f32,
    ) {
        let path = path.into();
        let cpu_cores = num_cpus::get();
        let max_concurrency = std::cmp::max(1, cpu_cores - 1); // Reserve one core for the system
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        let mut tasks = vec![];

        // Configure WalkDir based on the `recursive` flag
        let walker = WalkDir::new(&path);
        let walker = if recursive {
            walker
        } else {
            walker.min_depth(1).max_depth(1)
        }
        .into_iter();

        for entry in walker.filter_map(|e| e.ok()).filter(|e| e.path().is_file()) {
            match helpers::which_action(entry.clone()) {
                helpers::Actions::Convert => {
                    let sem_clone = semaphore.clone();
                    let entry_path = entry.into_path();

                    let task = tokio::task::spawn(async move {
                        let _permit = sem_clone
                            .acquire()
                            .await
                            .expect("Failed to acquire semaphore permit");
                        match convert_single_photo(
                            &entry_path,
                            quality,
                            lossless,
                            compression_factor,
                            should_resize,
                            noise_ratio,
                        )
                        .await
                        {
                            Ok(_) => {
                                info!(
                                    "\n{}\n",
                                    format!("Converted: {:?}", &entry_path)
                                        .bright_green()
                                        .bold()
                                );
                            }
                            Err(_) => {
                                match convert_single_photo(&entry_path, 75.0, 0, 0.0, false, 40.0)
                                    .await
                                {
                                    Ok(_) => {
                                        info!(
                                            "\n{}\n",
                                            format!("Converted: {:?}", &entry_path)
                                                .bright_green()
                                                .bold()
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            "\n{}\n",
                                            format!("Failed to convert: {:?} {:?}", &entry_path, e)
                                                .red()
                                                .bold()
                                        );
                                    }
                                }
                            }
                        }
                    });

                    tasks.push(task);
                }
                helpers::Actions::Copy => {
                    let sem_clone = semaphore.clone();
                    let entry_path = entry.into_path();

                    let task = tokio::spawn(async move {
                        let _permit = sem_clone
                            .acquire()
                            .await
                            .expect("Failed to acquire semaphore permit");
                        wio::copy_image_to_output_folder(&entry_path)
                            .await
                            .expect("Failed to copy image");
                    });

                    tasks.push(task);
                }
                helpers::Actions::Nothing => warn!(
                    "\n{}\n",
                    format!("Not a valid image file: {:?}", entry.path())
                        .yellow()
                        .bold()
                ),
            }
        }

        // Await all tasks to complete
        for task in tasks {
            task.await.expect("Task failed to complete");
        }

        wio::cleanup(path).expect("Failed to cleanup empty files.");
    }

    pub(crate) async fn convert_single_photo<P: Into<PathBuf>>(
        path: P,
        quality: f32,
        lossless: i32,
        compression_factor: f32,
        should_resize: bool,
        noise_ratio: f32,
    ) -> Result<(), WebpConverterError> {
        let path = path.into();
        let original_size = fs::metadata(&path)?.len() as f32;
        let target_size = match compression_factor as i32 {
            0 => 0,
            _ => (original_size / compression_factor) as i32,
        };

        let mut webp_dir = wio::get_or_create_output_directory(&path);

        if let Some(filename) = path.with_extension("webp").file_name() {
            webp_dir = webp_dir.join(filename);
        } else {
            webp_dir = webp_dir.join(path.file_name().ok_or_else(|| {
                Err::<PathBuf, WebpConverterError>(types::WebpConverterError::from(io::Error::new(
                    ErrorKind::NotFound,
                    "File not found!",
                )))
            })?);
        }

        wio::make_file_writable(&path)?;

        let img = image::open(&path)?; // Load the image synchronously to avoid async issues with WebPMemory
        let mut resized_img: DynamicImage = img.clone();
        // Prepare the file creation outside of the spawn_blocking to keep async operations out of the blocking context
        let webp_dir_clone = webp_dir.clone(); // Clone path for use in async context
        if webp_dir_clone.exists() {
            tokio::fs::remove_file(&webp_dir_clone).await?;
        }
        let file = tokio::fs::File::create(&webp_dir_clone).await?;
        let mut writer = BufWriter::new(file);

        if should_resize {
            resized_img = resize_image(img.clone());
        }

        let encode_task = decide_and_encode(
            img.clone(),
            resized_img.clone(),
            quality,
            lossless,
            noise_ratio,
            target_size,
        )
        .await?;
        // Finalize the file writing back in the async context
        if encode_task.len() != 0 {
            writer.write_all(&encode_task).await?;
        }

        Ok(())
    }

    pub async fn encode_webp(
        quality: f32,
        lossless: i32,
        noise_ratio: f32,
        target_size: i32,
        resized_img: DynamicImage,
    ) -> Result<Vec<u8>, WebpConverterError> {
        // Use spawn_blocking for the CPU-bound encoding task
        // Handle errors from spawn_blocking and encoding
        let encode_task = spawn_blocking(move || {
            let rgba_img: RgbaImage = resized_img.to_rgba8();

            // Configure WebP encoding
            let config = webp::WebPConfig {
                lossless,
                quality,
                method: 6,
                image_hint: libwebp_sys::WebPImageHint::WEBP_HINT_DEFAULT,
                target_size,
                target_PSNR: noise_ratio,
                segments: 4,
                sns_strength: 75,
                filter_strength: 60,
                filter_sharpness: 0,
                filter_type: 1,
                autofilter: 0,
                alpha_compression: 1,
                alpha_filtering: 1,
                alpha_quality: 90,
                pass: 3,
                show_compressed: 0,
                preprocessing: 2,
                partitions: 0,
                partition_limit: 2,
                emulate_jpeg_size: 0,
                thread_level: 1,
                low_memory: 0,
                near_lossless: 75,
                exact: 0,
                use_delta_palette: 0,
                use_sharp_yuv: 0,
                qmin: 0,
                qmax: 0,
            };

            let memory: WebPMemory =
                webp::Encoder::from_rgba(&rgba_img, resized_img.width(), resized_img.height())
                    .encode_advanced(&config)
                    .map_err(|_| {
                        Err::<WebPMemory, WebpConverterError>(WebpConverterError::from(
                            webp::WebPEncodingError::VP8_ENC_ERROR_BITSTREAM_OUT_OF_MEMORY,
                        ))
                    })?; // Handle encoding errors
            let memory_bytes: Vec<u8> = memory.to_vec();
            Ok::<Vec<u8>, WebpConverterError>(memory_bytes)
        })
        .await??; // Handle errors from spawn_blocking and encoding
        Ok(encode_task)
    }

    pub(crate) fn resize_image(image: DynamicImage) -> DynamicImage {
        let (width, height) = image.dimensions();

        // For images smaller than 700x700, return the original image.
        if width <= 700 && height <= 700 {
            return image;
        }

        // Calculate the new dimensions while maintaining the aspect ratio.
        let aspect_ratio = width as f32 / height as f32;
        let (new_width, new_height) = if width > height {
            let new_width = 700;
            let new_height = (700f32 / aspect_ratio).round() as u32;
            (new_width, new_height)
        } else if height > width {
            let new_height = 700;
            let new_width = (700f32 * aspect_ratio).round() as u32;
            (new_width, new_height)
        } else {
            // For square images or when width == height
            (700, 700)
        };

        // Resize the image using the Lanczos3 algorithm for high-quality results.
        image.resize_exact(new_width, new_height, FilterType::Lanczos3)
    }
}
