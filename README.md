## Rust WebP Image Converter
A high-performance, concurrent, multithreaded image converter written entirely in Rust. This tool allows for the bulk or single conversion of images to the compressed WebP format via a command-line interface (CLI). It's designed with static configurations derived from extensive experience in eCommerce image management, specifically tailored to the needs encountered during my tenure as an eCommerce Manager at Adam Medical Company.

![WebConverterLogo.png](src/assets/WebConverterLogo.png)
### Author
#### Ahmad Hamdi Emara

### Features
Concurrent & Multithreaded: Leverages Rust's powerful concurrency model for efficient image processing.
Bulk & Single Image Conversion: Supports processing an entire folder of images or a single image file.
Compressed WebP Format: Converts images to the WebP format, optimizing for high compression with minimal quality loss.
Static Configurations: Utilizes pre-defined settings optimized through professional experience in eCommerce image optimization.
CLI Interface: Easy-to-use command-line interface for straightforward operation.
Installation
(Include steps for installing Rust, if necessary, and building or installing your project.)

> Ensure you have Rust installed on your system. You can download it from https://rustup.rs/.

### Clone the repository to your local machine:

```sh
git clone https://github.com/theHamdiz/rust-webp-converter.git
```

### Navigate to the cloned repository directory:

```sh
cd webp_converter
```

### Build the project using Cargo:

```sh
cargo build --release
```

>The executable can be found in ./target/release/.

### Usage
To use the Rust WebP Image Converter, you can utilize the following command-line arguments:

`-p`:<PATH> *Path to an image file or folder containing images to convert.*   
`-l`:<LOSSLESS> *A boolean toggle (true/false) to indicate whether file compression should be lossless or lossy.*  
> Defaults to true.  
> 
`-q`:<QUALITY> *A number between 0 and 100 to indicate the quality of the compressed image.*  
> Defaults to 75.

`-c`:<COMPRESSIONFACTOR> *A number between 0 and 6 to indicate the compression factor of the compressed image.*  
> Defaults to 2.

`-V`:<VERSION> *Display the program version.*  
`-h`:<HELP> *Display the help menu with usage information.*  
`-r`:<RECURSIVE> *A boolean toggle (true/false) to indicate whether the program should recursively work on internal folders (Note: this feature is not yet implemented).*  

### Examples

Convert a single image with compression:

```sh
  ./webp_converter -p /path/to/image.png -c true
```

On `windows` that would be:

```sh
  .\webp_converter.exe -p "path\to\image" -c true
```
Convert all images in a folder without compression:

```sh
./webp_converter -p /path/to/folder -c false
```

Display the version of the program:

```sh
./webp_converter -V
```

### Contributing
>Feel free to contribute to this command line tool by submitting pull requests.

### License
>MIT License
