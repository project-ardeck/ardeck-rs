/*
libardeck - This is an all-in-one library for communicating with ardeck and integrating with computers.
Copyright (C) 2025 Project Ardeck

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

use serialport::{SerialPortInfo, SerialPortType};

/// コンピューターに接続されて利用可能なシリアルポートデバイス一覧を取得する
pub fn available_list() -> Vec<SerialPortInfo> {
    serialport::available_ports()
        .unwrap_or(Vec::new())
        .into_iter()
        .filter(|port| matches!(port.port_type, SerialPortType::UsbPort(_)))
        .collect()
}

/// デバイス一覧の抽出の実装
pub trait SerialPortInfoExt {
    fn arduino_only(self) -> Vec<SerialPortInfo>;
}

impl SerialPortInfoExt for Vec<SerialPortInfo> {
    /// デバイス一覧のうち、arduinoのベンダーコードを持つデバイスだけを抽出する
    fn arduino_only(self) -> Vec<SerialPortInfo> {
        self.into_iter()
            .filter(|port| {
                if let SerialPortType::UsbPort(info) = &port.port_type {
                    // 9025: Vendor code of Arduino SA
                    if info.vid == 9025 { true } else { false }
                } else {
                    false
                }
            })
            .collect()
    }
}
