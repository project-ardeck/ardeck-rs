use std::{path::PathBuf, sync::OnceLock};

static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn get_store_path() -> PathBuf {
    STORE_PATH.get().unwrap().to_path_buf()
}

#[derive(Debug, Default)]
pub struct StoreBuilder {
    /// 設定ファイルの保存先ディレクトリ
    path: PathBuf,
}

impl StoreBuilder {
    pub fn path(mut self, path: PathBuf) -> Self {
        assert!(path.is_dir(), "{} is not directory.", path.display());
        self.path = path;
        self
    }

    pub fn init(self) {
        STORE_PATH.set(self.path).unwrap();
    }
}
