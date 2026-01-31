use std::{path::PathBuf, sync::OnceLock};

use serde::{Serialize, de::DeserializeOwned};

use crate::config::ConfigFile;

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

pub trait StoreTrait: Serialize + DeserializeOwned + ConfigFile + Clone + Send + Sync {
    fn path() -> PathBuf {
        STORE_PATH.get().unwrap().join(Self::name())
    }

    fn load() -> Result<Self, std::io::Error> {
        if !Self::path().try_exists()? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File is Not found.",
            ));
        }

        Ok(Self::default())
    }
}
