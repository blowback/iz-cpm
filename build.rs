// Builds the VideoBeast shim and the vendored beastem sources when feature
// `videobeast` is on. The stock build does nothing here.

fn main() {
    #[cfg(feature = "videobeast")]
    videobeast::build();
}

#[cfg(feature = "videobeast")]
mod videobeast {
    use std::process::Command;

    const SOURCES: [&str; 3] = [
        "shim/vbshim.cpp",
        "vendor/beastem/videobeast.cpp",
        "vendor/beastem/assets.cpp",
    ];

    fn pkg_config(args: &[&str]) -> Vec<String> {
        let out = Command::new("pkg-config")
            .args(args)
            .arg("sdl2")
            .output()
            .expect("feature videobeast needs pkg-config and SDL2 development files");
        if !out.status.success() {
            panic!(
                "pkg-config {:?} sdl2 failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        }
        String::from_utf8(out.stdout)
            .expect("pkg-config output is not UTF-8")
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    pub fn build() {
        let mut build = cc::Build::new();
        build.cpp(true).std("c++17").warnings(false);
        for flag in pkg_config(&["--cflags"]) {
            build.flag(&flag);
        }
        for src in SOURCES {
            build.file(src);
        }
        build.compile("vbshim");

        for flag in pkg_config(&["--libs-only-L"]) {
            if let Some(dir) = flag.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={}", dir);
            }
        }
        println!("cargo:rustc-link-lib=SDL2");

        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=shim/vbshim.h");
        println!("cargo:rerun-if-changed=vendor/beastem/videobeast.hpp");
        println!("cargo:rerun-if-changed=vendor/beastem/assets.hpp");
        for src in SOURCES {
            println!("cargo:rerun-if-changed={}", src);
        }
    }
}
