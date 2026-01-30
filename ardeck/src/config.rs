use serde::{Serialize, de::DeserializeOwned};

/// 設定ファイルを定義する
///
/// # Example
///
/// ```
/// struct MyConfig {
///     name: String,
///     age: u32,
/// }
///
/// impl Default for MyConfig {
///     fn default() -> Self {
///         Self {
///             name: "John Doe".into(),
///             age: 42,
///         }
///     }
/// }
///
/// impl ConfigFile for MyConfig {
///     fn name() -> &'static str {
///         "my_config.json"
///     }
/// }
/// ```
pub trait ConfigFile: Serialize + DeserializeOwned + Default + Clone + Send + Sync {
    fn name() -> &'static str;
}
