use std::{
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc},
    thread,
};

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

impl SwitchInfo {
    pub fn kind(&self) -> SwitchKind {
        self.kind
    }

    pub fn pin(&self) -> u8 {
        self.pin
    }

    pub fn state(&self) -> u16 {
        self.state
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}

/// USBポートの情報
#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    vid: u16,
    pid: u16,
    serial_number: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
}

impl PortInfo {
    pub fn vid(&self) -> u16 {
        self.vid
    }

    pub fn pid(&self) -> u16 {
        self.pid
    }

    pub fn serial_number(&self) -> Option<&String> {
        self.serial_number.as_ref()
    }

    pub fn manufacturer(&self) -> Option<&String> {
        self.manufacturer.as_ref()
    }

    pub fn product(&self) -> Option<&String> {
        self.product.as_ref()
    }
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

impl DeviceInfo {
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    pub fn port_info(&self) -> &PortInfo {
        &self.port_info
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
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

impl ProfileItem {
    pub fn switch_type(&self) -> SwitchKind {
        self.switch_type
    }

    pub fn switch_id(&self) -> usize {
        self.switch_id
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }
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

impl Profile {
    pub fn version(&self) -> usize {
        self.version
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    pub fn maps(&self) -> &[ProfileItem] {
        &self.maps
    }
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

pub struct CobsReceiver {
    buffer: Vec<u8>,
}

impl CobsReceiver {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        for &byte in data {
            if byte == 0x00 {
                if self.buffer.is_empty() {
                    // 空フレーム
                    continue;
                }

                // https://doc.rust-lang.org/std/mem/fn.take.html
                // 所有権ではなくて値そのものを移動してくるイメージ？移動された元の変数は空になる。
                let encoded = std::mem::take(&mut self.buffer);

                match dec_cobs(&encoded) {
                    Some(decoded) => {
                        frames.push(decoded);
                    }
                    None => {
                        // 不正なCOBSフレーム
                        // ログ等
                    }
                }
            } else {
                self.buffer.push(byte);
            }
        }

        frames
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct WorkerStatus {
    worker_write: bool,
    worker_read: bool,
    worker_action: bool,
}

#[derive(Debug, Clone, Copy)]
enum ControlMessage {
    /// すべてのスイッチの情報を送信するように要求する
    RequestAllSwitchInfo,
    /// スイッチの情報を連続的に送信するモードを開始するように要求する
    RequestStartContinuousMode,
    /// スイッチの情報を、スイッチの状態が変化したときに限って
    /// 連続的に送信するモードを開始するように要求する
    RequestStartTriggeredContinuousMode,
    /// 連続的に送信するモード(上二つ)のいずれかのモードを停止するように要求する
    RequestStopContinuousMode,
}

#[derive(Debug, Clone)]
enum ActionMessage {
    SwitchInfos(Vec<SwitchInfo>),
}

#[derive(Debug, thiserror::Error)]
enum ConnectionError {
    #[error("{0}")]
    CommunicationError(#[from] serialport::Error),
}

pub struct Connection {
    worker_status: Arc<Mutex<WorkerStatus>>,
    exit_flag: Arc<AtomicBool>,
    control_tx: mpsc::Sender<ControlMessage>,
    // action_rx: mpsc::Receiver<ActionMessage>,
}

impl Connection {
    pub fn new<'a>(
        path: impl Into<std::borrow::Cow<'a, str>>,
        baud_rate: u32,
    ) -> Result<Self, ConnectionError> {
        // 全スレッド共有で処理状況のステータス保管用
        let worker_status = Arc::new(Mutex::new(WorkerStatus::default()));
        let exit_flag = Arc::new(AtomicBool::new(false));

        let serialport = serialport::new(path, baud_rate).open()?;

        // 外から送信用への橋渡し
        let (control_tx, control_rx) = std::sync::mpsc::channel::<ControlMessage>();
        // 受信から処理への橋渡し
        let (action_tx, action_rx) = std::sync::mpsc::channel::<ActionMessage>();

        // 送信用のスレッドを作る
        let worker_status_cloned = worker_status.clone();
        let serialport_cloned = serialport.try_clone()?;
        let exit_flag_cloned = exit_flag.clone();
        thread::spawn(move || {
            let worker_status = worker_status_cloned;
            let mut serialport = serialport_cloned;
            let control_rx = control_rx;
            let exit_flag = exit_flag_cloned;

            while let Ok(cmd) = control_rx.recv() {
                if exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
        });

        // 受信用のスレッドを作る
        let worker_status_cloned = worker_status.clone();
        let serialport_cloned = serialport.try_clone()?;
        let exit_flag_cloned = exit_flag.clone();
        thread::spawn(move || {
            let worker_status = worker_status_cloned;
            let mut serialport = serialport_cloned;
            let action_tx = action_tx;
            let exit_flag = exit_flag_cloned;
            let mut cobs = CobsReceiver::new();

            let mut buf = [0u8; 8];

            while let Ok(read_len) = serialport.read(&mut buf) {
                let data = &buf[0..read_len];

                let decoded = cobs.push(data);
                let switch_infos: Vec<SwitchInfo> = decoded
                    .iter()
                    .filter_map(|data| raw_to_switch_info(data))
                    .collect();

                if let Err(e) = action_tx.send(ActionMessage::SwitchInfos(switch_infos)) {
                    // TODO: エラーハンドリング
                };

                if exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
        });

        // 処理スレッド
        let worker_status_cloned = worker_status.clone();
        let exit_flag_cloned = exit_flag.clone();
        thread::spawn(move || {
            let worker_status = worker_status_cloned;
            let action_rx = action_rx;
            let exit_flag = exit_flag_cloned;

            while let Ok(cmd) = action_rx.recv() {
                if exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
        });

        Ok(Self {
            worker_status,
            exit_flag,
            control_tx,
            // action_rx,
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec() {
        let mut receiver = CobsReceiver::new();

        let result = receiver.push(&[
            0x02, 0x01, 0x00, // 単一のデータ(Data1)
            0x04, 0x11, // 次のデータの断片(Data2)
        ]);

        assert_eq!(result, vec![vec![0x01],]);

        let result = receiver.push(&[
            0x22, 0x33, 0x00, // (Data2)
            0x02, 0xAA, 0x00, // (Data3)
        ]);

        assert_eq!(result, vec![vec![0x11, 0x22, 0x33], vec![0xAA],]);
    }
}
