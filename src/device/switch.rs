/*
libardeck - This is an all-in-one library for communicating with ardeck and integrating with computers.
Copyright (C) 2026 Project Ardeck

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use serde::{Deserialize, Serialize};

/// Arduinoに接続されているスイッチの種類を示す列挙型
#[derive(Clone, Copy, Deserialize, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SwitchKind {
    /// デジタルスイッチ ex: タクトスイッチ, トグルスイッチ
    Digital = 0,
    /// アナログスイッチ ex: ポテンションメーター, アナログジョイスティック
    Analog = 1,
}

/// デバイスによって押されたスイッチの情報を保持する構造体
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwitchInfo {
    /// スイッチの種類
    kind: SwitchKind,
    /// スイッチが接続されているArduino上のピン番号
    pin: u8,
    /// スイッチの状態を表す数値
    state: u16,
    /// データが取得された時刻(正確には)
    timestamp: i64,
}
