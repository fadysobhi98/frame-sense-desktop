# Third-party components

## rawler (LGPL-2.1)
The RAW decoding engine (`desktop/src-tauri/raw-engine`, shipped as
`framesense_raw.dll`) links the `rawler` crate
(https://github.com/dnglab/dnglab, crates.io/crates/rawler), which is
licensed under the GNU Lesser General Public License v2.1.

Frame Sense keeps rawler in its own separate DLL that the app loads at
runtime, so the component stays replaceable: anyone can rebuild
`framesense_raw.dll` from the source in this repository
(`desktop/src-tauri/raw-engine`) and swap the file next to the app.
rawler's own source is available at the link above and on crates.io.

All other dependencies are MIT / Apache-2.0 / BSD licensed Rust and npm
crates; see Cargo.toml and package.json.

## SeetaFace frontal face-detection model (seeta_fd_frontal_v1.0.bin)

Bundled in desktop/src-tauri/models/ and compiled into the app. From the
SeetaFace Engine project (SeetaTech), BSD-2-Clause license, loaded through
the rustface crate (BSD-2-Clause). Used only for local on-device face
counting; no image ever leaves the user's machine.
