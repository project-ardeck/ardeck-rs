use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serialport::{SerialPort, SerialPortType};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Serialport error: {0}")]
    Serialport(#[from] serialport::Error),

    #[error("Std io error: {0}")]
    StdIo(#[from] std::io::Error),

    #[error("Failed to parse the received data.")]
    ParseError,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SwitchKind {
    /// デジタルスイッチ。タクトスイッチ、トグルスイッチなど。
    #[default]
    Digital,
    /// アナログスイッチ。ポテンショメータ、ジョイスティックなど。
    Analog,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwitchInfo {
    kind: SwitchKind,
    /// スイッチが接続されているピン番号。
    pin: u8,
    /// スイッチの状態を表す値。
    state: u16,
    /// データの受信時刻を表すタイムスタンプ
    timestamp: Timestamp,
}

/// USBポートの情報
#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl From<serialport::UsbPortInfo> for PortInfo {
    fn from(value: serialport::UsbPortInfo) -> Self {
        PortInfo {
            vid: value.vid,
            pid: value.pid,
            serial_number: value.serial_number,
            manufacturer: value.manufacturer,
            product: value.product,
        }
    }
}

/// コンピューターに接続されて利用可能なシリアルポートデバイスの情報
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// ポート名
    port_name: String,
    /// 取得できたポート情報
    port_info: PortInfo,
    /// ポート情報から生成されたデバイスID
    device_id: String,
}

/// デバイス一覧の実装
#[allow(dead_code)]
trait DeviceInfosFilterTrait {
    /// デバイス一覧のうち、arduinoのベンダーコードを持つデバイスだけを抽出する。
    /// # Example
    /// ```
    /// let device = ardeck::device::available_list().arduino_only();
    /// ```
    fn arduino_only(self) -> Vec<DeviceInfo>;
}

impl DeviceInfosFilterTrait for Vec<DeviceInfo> {
    fn arduino_only(self) -> Vec<DeviceInfo> {
        self.into_iter()
            // 9025: Arduino LA のベンダーID
            .filter(|port| port.port_info.vid == 9025)
            .collect()
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// プロファイルフォーマットバージョン: 1
    version: usize,
    /// プロファイル名
    profile_name: String,
    /// スイッチとアクションの紐づけ設定の配列
    maps: Vec<ProfileItem>,
}

/// COBS形式のデータを生バイト値に変換する。
///
/// 正しく変換できなかった場合、[`None`]を返す。
fn dec_cobs(cobs_bytes: impl AsRef<[u8]>) -> Option<Vec<u8>> {
    let mut cobs_bytes = cobs_bytes.as_ref().to_vec();
    if *cobs_bytes.last()? != 0 {
        return None;
    }

    let mut i = 0;
    loop {
        let i_val = *cobs_bytes.get(i)?;

        cobs_bytes[i] = 0;

        if i_val == 0 {
            break;
        } else {
            i += i_val as usize;
        }
    }

    Some(cobs_bytes[1..cobs_bytes.len() - 1].to_vec())
}

fn raw_to_switch_info(bytes: impl AsRef<[u8]>) -> Option<SwitchInfo> {
    let bytes = bytes.as_ref().to_vec();

    #[cfg(not(test))]
    let timestamp = Timestamp::now();

    #[cfg(test)]
    let timestamp = Timestamp::default();

    // switch kind
    match bytes.get(0)? & 0x80 {
        // Digital Switch
        0 => {
            if bytes.len() == 1 {
                Some(SwitchInfo {
                    kind: SwitchKind::Digital,
                    pin: (bytes[0] & 0b01111110) >> 1,
                    state: (bytes[0] & 1) as u16,
                    timestamp,
                })
            } else {
                None
            }
        }
        // Analog Switch
        1 => {
            if bytes.len() == 2 {
                Some(SwitchInfo {
                    kind: SwitchKind::Analog,
                    pin: (bytes[0] & 0b01111100) >> 2,
                    state: ((bytes[0] as u16 & 0b11) << 8) | bytes[1] as u16,
                    timestamp,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// デバイスのハードウェア固有番号を使用して、識別番号を作成する
fn make_device_id(port_info: &PortInfo) -> String {
    if let Some(serial_number) = &port_info.serial_number {
        format!(
            "{:04X}-{:04X}-{}",
            port_info.vid, port_info.pid, serial_number
        )
    } else {
        format!("{:04X}-{:04X}", port_info.vid, port_info.pid)
    }
}

/// 接続可能なUSB Port一覧を取得する
///
/// # Example
/// ```
/// let device = ardeck::device::available_list();
/// ```
pub fn available_list() -> Vec<DeviceInfo> {
    serialport::available_ports()
        .unwrap_or(Vec::new())
        .into_iter()
        .filter_map(|port| match port.port_type {
            SerialPortType::UsbPort(e) => {
                let port_info: PortInfo = e.into();
                let device_id = make_device_id(&port_info);
                Some(DeviceInfo {
                    port_name: port.port_name.clone(),
                    port_info,
                    device_id,
                })
            }
            _ => None,
        })
        .collect()
}

pub struct Communication {
    device_info: DeviceInfo,
    serialport: Box<dyn SerialPort>,
}

impl Communication {
    fn new(device_info: DeviceInfo) -> Result<Self, Error> {
        let serialport = serialport::new(&device_info.port_name, 9600).open()?;

        Ok(Self {
            device_info,
            serialport,
        })
    }

    /// Ardeckコマンド スイッチ情報全取得 0xFF
    fn request_switch_info_all(&mut self) -> Result<SwitchInfo, Error> {
        let _wrote_bytes = self.serialport.write(&[0xFF])?;

        let mut buf = [0u8; 16];
        let _read = self.serialport.read(&mut buf)?;

        let Some(bytes) = dec_cobs(&buf) else {
            return Err(Error::ParseError);
        };

        let Some(info) = raw_to_switch_info(bytes) else {
            return Err(Error::ParseError);
        };

        Ok(info)
    }
}

// pub struct Decoder {
//     buf: Vec<u8>,
// }

// impl Decoder {
//     pub fn new() -> Self {
//         Self { buf: Vec::new() }
//     }

//     /// COBSエンコードされたバイトデータを蓄積する
//     pub fn receive(&mut self, data: &[u8]) {
//         self.buf.append(&mut data.as_ref().to_vec());
//     }

//     /// 蓄積されたバイトデータをCOBSエンコードする。
//     ///
//     /// 1度デコードが完了した時点で完成品を返却します。
//     /// デコードに失敗したら[`None`]が返ります。
//     pub fn process_buffer(&mut self) -> Option<Vec<u8>> {
//         // 0までを切り取ってスライスにする。なければNoneを返す
//         let mut buf = self
//             .buf
//             .drain(0..=self.buf.iter().position(|x| *x == 0)?)
//             .as_slice()
//             .to_vec();

//         log::trace!("Found one set: {:?}", buf);

//         // 切り取ったデータをデコードする
//         let mut i = 0;
//         loop {
//             let i_val = *buf.get(i)?;

//             buf[i] = 0;

//             if i_val == 0 {
//                 break;
//             } else {
//                 i += i_val as usize;
//             }
//         }

//         let buf = &buf[1..buf.len() - 1];

//         log::trace!("Decoded: {:?}", buf);

//         // チェックサム
//         let sum = buf.last()?; // 受け取った計算済み合計値
//         let payload = &buf[0..buf.len() - 1]; // 受け取ったデータのペイロード
//         let mut now_sum: u8 = 0; // 今から計算する合計値
//         for byte in payload.iter() {
//             now_sum = now_sum.wrapping_add(*byte);
//         }

//         if *sum == now_sum {
//             Some(payload.to_vec())
//         } else {
//             log::debug!("SUM error: {} != {}", sum, now_sum);
//             None
//         }
//     }

//     #[cfg(test)]
//     pub fn get_buf(&mut self) -> Vec<u8> {
//         self.buf.clone()
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn dec() {}
// }
