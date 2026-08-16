//! Punto de entrada del ejecutable de escritorio.

// Evita que se abra una consola extra en Windows al compilar en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tabs_repo_lib::run();
}
