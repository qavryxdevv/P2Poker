//! Build-time resources.
//!
//! Windows takes an application's icon from a resource compiled **into** the
//! executable, not from a file beside it — which matters here more than usual:
//! this client is one portable binary, and an icon that lived in a second file
//! would be an icon that vanished the moment somebody copied the thing.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "Decentralised peer-to-peer poker");
        res.set("ProductName", "p2p-poker");
        res.set("LegalCopyright", "");
        // A failure here is not worth failing the build over: the client runs
        // perfectly well with the default icon, and refusing to compile because
        // of a picture would be the wrong trade.
        if let Err(e) = res.compile() {
            println!("cargo:warning=the icon was not embedded: {e}");
        }
    }
}
