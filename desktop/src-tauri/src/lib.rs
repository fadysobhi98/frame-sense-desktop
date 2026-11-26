mod core;

use tauri::command;

#[command]
fn create_project(
    base_directory: String,
    client_name: String,
    event_type: String,
    event_date: String,
) -> Result<core::CreateProjectResult, String> {
    core::create_project(&base_directory, &client_name, &event_type, &event_date)
}

#[command]
fn import_camera_files(
    project_path: String,
    source_path: String,
    camera_id: String,
) -> Result<core::ImportResult, String> {
    core::import_camera_files(&project_path, &source_path, &camera_id)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            create_project,
            import_camera_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frame Sense");
}
