# file-utils

File utilities: errors (`FileError`), drag & drop
(accepts multiple files, returns `Vec`), native dialogs
via `rfd` (no egui).

Key functions:
- [`FileError`] (IO, format, etc.)
- [`load_image`] (PNG/JPEG/etc. → `RgbaBuf`)
- [`open_file_picker`] / [`save_file_picker`] (rfd)

Apps import these helpers in `ui/dialogs.rs` and
in load/export logic.
