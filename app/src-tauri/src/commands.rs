//! Thin Tauri wrappers over `Backend`. Slow calls run off the main thread.

use crate::backend::{Backend, OpRequest, PlanView};
use roster_core::view::{ChangeView, FirstRunView, OverviewView};
use std::sync::Arc;
use tauri::{AppHandle, State};

async fn blocking<T, F>(b: &State<'_, Arc<Backend>>, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Backend) -> Result<T, String> + Send + 'static,
{
    let b = Arc::clone(b);
    tauri::async_runtime::spawn_blocking(move || f(&b))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn overview(app: AppHandle, b: State<'_, Arc<Backend>>) -> Result<OverviewView, String> {
    let v = blocking(&b, |b| b.overview()).await?;
    crate::tray::update(&app, &v);
    Ok(v)
}

#[tauri::command]
pub async fn first_run(b: State<'_, Arc<Backend>>) -> Result<FirstRunView, String> {
    blocking(&b, |b| b.first_run()).await
}

#[tauri::command]
pub async fn preview(b: State<'_, Arc<Backend>>, req: OpRequest) -> Result<PlanView, String> {
    blocking(&b, move |b| b.preview(req)).await
}

#[tauri::command]
pub async fn apply(b: State<'_, Arc<Backend>>, token: u64) -> Result<Option<u64>, String> {
    blocking(&b, move |b| b.apply(token)).await
}

#[tauri::command]
pub async fn changes(b: State<'_, Arc<Backend>>) -> Result<Vec<ChangeView>, String> {
    blocking(&b, |b| b.changes()).await
}

#[tauri::command]
pub async fn undo(b: State<'_, Arc<Backend>>, id: u64) -> Result<(), String> {
    blocking(&b, move |b| b.undo(id)).await
}

#[tauri::command]
pub async fn undo_all(b: State<'_, Arc<Backend>>) -> Result<usize, String> {
    blocking(&b, |b| b.undo_all()).await
}

#[tauri::command]
pub async fn sign_in(b: State<'_, Arc<Backend>>, name: String) -> Result<(), String> {
    blocking(&b, move |b| b.sign_in(&name)).await
}

#[tauri::command]
pub async fn open_terminal_as(b: State<'_, Arc<Backend>>, name: String) -> Result<(), String> {
    blocking(&b, move |b| b.open_terminal_as(&name)).await
}
