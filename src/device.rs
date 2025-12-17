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

pub mod switch;

use serialport::{SerialPortType, UsbPortInfo};

/// コンピューターに接続されて利用可能なシリアルポートデバイスの情報
pub struct AvailableDeviceInfo {
    /// ポート名
    pub port_name: String,
    /// 取得できたポート情報
    pub usb_port_info: UsbPortInfo,
    /// ポート情報から生成されたデバイスID
    pub device_id: String,
}

impl AvailableDeviceInfo {
    /// 接続可能なUSB Port一覧を取得する
    /// # Example
    /// ```
    /// let device = AvailableDeviceInfo::list();
    /// ```
    pub fn list() -> Vec<Self> {
        serialport::available_ports()
            .unwrap_or(Vec::new())
            .into_iter()
            .filter_map(|port| match &port.port_type {
                SerialPortType::UsbPort(e) => Some(Self {
                    port_name: port.port_name.clone(),
                    usb_port_info: e.clone(),
                    device_id: "TODO".into(), // TODO: ID生成
                }),
                _ => None,
            })
            .collect()
    }
}

/// デバイス一覧の実装
pub trait AvailableDeviceInfoList {
    fn arduino_only(self) -> Vec<AvailableDeviceInfo>;
}

impl AvailableDeviceInfoList for Vec<AvailableDeviceInfo> {
    /// デバイス一覧のうち、arduinoのベンダーコードを持つデバイスだけを抽出する
    /// # Example
    /// ```
    /// let device = AvailableDeviceInfo::list().arduino_only();
    /// ```
    fn arduino_only(self) -> Vec<AvailableDeviceInfo> {
        self.into_iter()
            .filter(|port| {
                // 9025: Arduino LA のベンダーID
                if port.usb_port_info.vid == 9025 {
                    true
                } else {
                    false
                }
            })
            .collect()
    }
}
