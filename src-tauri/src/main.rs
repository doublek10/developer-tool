// Hides the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analyzer;
mod auth;
mod cv;
mod db;
mod dblab;
mod error;
mod files;
mod license;
mod logging;
mod pdfstudio;
mod system;
mod testlab;
mod vault;
mod website;
mod workspace;

use auth::Session;
use db::Db;
use std::sync::Mutex;

/// Single timestamp format used everywhere, so logs sort as strings.
pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn main() {
    let conn = db::open().expect("the local database could not be created");
    logging::write(&conn, "info", "SYSTEM", "DevWorkstation started", None);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Db(Mutex::new(conn)))
        .manage(Session::default())
        .invoke_handler(tauri::generate_handler![
            // session
            auth::has_account,
            auth::register,
            auth::sign_in,
            auth::sign_out,
            auth::current_user,
            auth::change_password,
            auth::change_username,
            // licensing
            license::license_status,
            // dashboard + system
            system::dashboard,
            system::system_resources,
            system::is_online,
            system::export_report,
            // cv builder
            cv::cv_list,
            cv::cv_load,
            cv::cv_save,
            cv::cv_duplicate,
            cv::cv_delete,
            cv::cv_export_pdf,
            // pdf studio
            pdfstudio::pdf_inspect,
            pdfstudio::pdf_extract_text,
            pdfstudio::pdf_delete_pages,
            pdfstudio::pdf_rotate_pages,
            pdfstudio::pdf_merge,
            pdfstudio::pdf_split,
            // website lab
            website::website_scan,
            // code analyzer
            analyzer::analyze_project,
            // database lab
            dblab::db_analyze_sqlite,
            dblab::db_backup_sqlite,
            dblab::db_apply_repair,
            dblab::db_run_select,
            // test lab + terminal
            testlab::test_detect,
            testlab::test_run,
            testlab::code_run_file,
            testlab::terminal_run,
            // workspace
            workspace::projects_list,
            workspace::project_save,
            workspace::project_delete,
            workspace::servers_list,
            workspace::server_save,
            workspace::server_delete,
            workspace::server_reachable,
            workspace::databases_list,
            workspace::database_save,
            workspace::database_delete,
            // files
            files::fs_list,
            files::fs_home,
            files::fs_read_text,
            files::fs_write_text,
            files::fs_create_dir,
            files::fs_rename,
            files::fs_delete,
            files::fs_copy,
            files::fs_search,
            files::fs_zip,
            files::fs_unzip,
            // vault
            vault::vault_list,
            vault::vault_store,
            vault::vault_delete,
            vault::vault_verify,
            // logs
            logging::logs_query,
            logging::logs_export,
            logging::logs_clear,
        ])
        .run(tauri::generate_context!())
        .expect("DevWorkstation failed to start");
}
