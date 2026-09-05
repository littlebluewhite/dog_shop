fn main() {
    // migrations/ 有變動時重新編譯，讓 sqlx::migrate! 內嵌的內容跟著更新
    println!("cargo:rerun-if-changed=migrations");
}
