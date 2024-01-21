use winres;

fn main() {
    build_icon();
}

fn build_icon() {
     if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/assets/WebpConverterLogo.ico");
            // .set("WebpConverterByAhmadHamdi", "WebpConverter.exe")
            // .set_version_info(winres::VersionInfo::PRODUCTVERSION, 0x0001000000000000);
        res.compile().unwrap();
    }
}