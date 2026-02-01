use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::OnceLock,
};

use serde::{Serialize, de::DeserializeOwned};

use crate::config::ConfigFile;

static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn get_store_path() -> PathBuf {
    STORE_PATH.get().unwrap().to_path_buf()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Io error")]
    Io(#[from] std::io::Error),
    #[error("Serde error")]
    Serde(#[from] serde_json::Error),
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
        // TODO: ディレクトリを作成する
    }
}

pub trait StoreTrait: Serialize + DeserializeOwned + ConfigFile + Clone + Send + Sync {
    /// 設定ファイルまでのフルパスを取得する
    fn path() -> PathBuf {
        STORE_PATH.get().unwrap().join(Self::name())
    }

    /// 設定を読み込む
    ///
    /// # Example
    ///
    /// ```
    /// let mut my_config = MyConfig::load().unwrap_or_default();
    /// my_config.age += 1;
    /// my_config.save().unwrap();
    /// ```
    fn load() -> Result<Self, Error> {
        let file = std::fs::File::open(Self::path())?;
        let reader = std::io::BufReader::new(file);

        Ok(serde_json::from_reader::<
            std::io::BufReader<std::fs::File>,
            Self,
        >(reader)?)
    }

    /// 設定を保存する
    ///
    /// # Example
    ///
    /// ```
    /// let mut my_config = MyConfig::load().unwrap_or_default();
    /// my_config.age += 1;
    /// my_config.save().unwrap();
    /// ```
    fn save(self) -> Result<Self, Error> {
        let file = std::fs::File::create(Self::path())?;
        let mut writer = std::io::BufWriter::new(file);

        let file_str = serde_json::to_string_pretty(&self)?;
        writer.write_all(file_str.as_bytes())?;

        Ok(self)
    }
}
