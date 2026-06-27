// 发布版隐藏额外的控制台窗口（Windows），不要删除
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    demiurge_lib::run()
}
