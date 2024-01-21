
use image::io::Reader as ImageReader;
use image::{ImageFormat};
use std::{fs, io};
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use log::{info, error, warn};
use env_logger;
use std::env;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();
    let directory_path = env::args().nth(1).unwrap_or_else(|| {
        info!("Please provide a directory path:");
        io::stdout().flush().unwrap(); // Make sure the prompt is displayed immediately
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string() // Remove the newline character at the end
    });

    let path = helpers::process_path_for_os(directory_path);
    let path_buff = PathBuf::from(path);

    if ! path_buff.exists() { error!("Path does not exist, terminating...."); return; }

    println!("Checking path: {:?}", path_buff);

    if path_buff.is_dir(){
        info!("Directory Detected Working on it...");
        converter::convert_images_to_webp(path_buff);
    }else{
        info!("Single Image File Detected...");
        converter::convert_single_photo(path_buff).await;
    }
}

pub(crate) mod helpers{
    use walkdir::DirEntry;
    use super::*;

    // pub enum RunMode{
    //     Gui,
    //     Cli
    // }

    pub(crate) enum Actions{
        Convert,
        Copy,
        Nothing
    }
    pub(crate) fn which_action(path: DirEntry) -> Actions {
        // Check if the file is an image and should be converted or copied.
        let p = path.path().to_string_lossy().to_string().replace('"', "");
        let path = PathBuf::from(&p);
        info!("Path: {:?}", path);
        // info!("Extension: {:?}", path.extension());
        match path.extension().and_then(|e| e.to_str()) {
            Some("jpg") | Some("jpeg") | Some("png") | Some("tiff") | Some("bmp") | Some("avif") => Actions::Convert,
            Some("webp") => Actions::Copy,
            _ => Actions::Nothing,
        }
    }



    pub(crate) fn process_path_for_os<S: Into<String>>(path: S) -> String {
        let mut path = path.into();
        info!("Path: {}", path);
        #[cfg(windows)]
        {
            // For Windows, if the path contains spaces and is not already quoted, quote it.
            if path.contains(' ') && !path.starts_with('"') && !path.ends_with('"') {
                warn!("Path contains spaces, quoting it....");
                // return format!("\"{}\"", path);
            }
            path = path.replace("/", "\\"); // Convert Unix-style slashes to Windows-style.
            path = path.replace('\\', "\\"); // Convert spaces to windows separators.
        info!("Path after modifications: {}", path);
            path
        }
        #[cfg(not(windows))]
        {
            // For Unix-like systems, ensure the path is escaped properly.
            // This simplistic approach handles spaces; adapt as needed for other special characters.
            path.replace(" ", "\\ ")
        }
    }
}

pub(crate) mod wio{
    use super::*;
    pub(crate) async fn copy_image_to_output_folder(p0: &Path) -> Result<u64, io::Error> {
        let filename = p0.file_name().unwrap();
        let copy_path = get_output_directory(p0).join(filename);
        info!("Copying: {:?} to {:?}", p0, copy_path);
        fs::copy(p0, copy_path)
    }

    pub(crate) fn get_output_directory(path: &Path) -> PathBuf {
        // Create the "webp_converter" directory inside the original image's directory
        let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
        let webp_dir = parent_dir.join("webp converter output");
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

pub(crate) mod converter{
    use std::sync::Arc;
    use super::*;
    pub(crate) fn convert_images_to_webp<P: Into<PathBuf>>(path: P) {
        let path = path.into();
        let cpu_cores = num_cpus::get();
        let max_concurrency = std::cmp::max(1, cpu_cores - 1);
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        let mut tasks = vec![];

        for entry in WalkDir::new(path.clone())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            match helpers::which_action(entry.clone()) {

                helpers::Actions::Convert => {
                    let sem_clone = semaphore.clone();

                    let task = tokio::spawn(async move {
                        let _permit = sem_clone.acquire().await.expect("Failed to acquire semaphore permit");
                        convert_single_photo(entry.path()).await;
                    });

                    tasks.push(task);

                },
                helpers::Actions::Copy => {
                    let sem_clone = semaphore.clone();

                    let task = tokio::spawn(async move {
                        let _permit = sem_clone.acquire().await.expect("Failed to acquire semaphore permit");
                        wio::copy_image_to_output_folder(entry.path()).await.unwrap();
                    });

                    tasks.push(task);
                },
                helpers::Actions::Nothing => warn!("Not a valid image file: {:?}", entry.path()),
            }

        }
    }

    pub(crate) async fn convert_single_photo<P: Into<PathBuf>>(path: P){
        let path = path.into();
        let filename = path.file_name().unwrap();
        wio::make_file_writable(&path).unwrap_or_else(|_| {});
        let img = ImageReader::open(path.clone()).unwrap().decode().unwrap();
        let webp_dir = wio::get_output_directory(&path).join(filename);
        img.save_with_format(webp_dir, ImageFormat::WebP).unwrap();
        info!("Converted: {:?}", path);
    }
}





