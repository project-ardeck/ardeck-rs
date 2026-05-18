use serde::{Deserialize, Serialize};

use crate::device::switch::SwitchKind;

/// ボタンの押下やアナログスイッチの操作をトリガーにし、キーボード入力等のアクションを発生させるための設定
///
/// ```json
/// [
///     {
///         switch_type: "Digital",
///         switch_id: 0,
///         plugin_id: "Keyboard",
///         action_id: "D"
///     },
///     ...
/// ]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItem {
    switch_type: SwitchKind,
    switch_id: usize,
    plugin_id: String,
    action_id: String,
}
