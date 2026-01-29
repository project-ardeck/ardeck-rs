use std::fmt::Display;

use chrono::{Local, Utc};

use crate::device::switch::SwitchInfo;

// TODO: エラー列挙作る？

/// cobs形式のデータを生のバイト列へデコードします。失敗したら `None` を返します。
///
/// # Example
///
/// ```
/// assert_eq!(dec_cobs(vec![01, 01, 00]), Some(vec![00]));
/// assert_eq!(dec_cobs(vec![01, 01, 01, 00]), Some(vec![00, 00]));
/// assert_eq!(dec_cobs(vec![01, 02, 11, 01, 00]), Some(vec![00, 11, 00]));
/// assert_eq!(
///     dec_cobs(vec![03, 11, 22, 02, 33, 00]),
///     Some(vec![11, 22, 00, 33])
/// );
/// ```
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

/// 生のバイト列をパースします。失敗したら `None` を返します。
///
/// パースの挙動の詳細については下記URL `PROTOCOL.md` を参照ください。
///
/// https://github.com/project-ardeck/ardeck-sketch/blob/main/PROTOCOL.md
pub fn raw_to_switch_info(bytes: impl AsRef<[u8]>) -> Option<SwitchInfo> {
    let bytes = bytes.as_ref().to_vec();

    #[cfg(not(test))]
    let timestamp_micros = Utc::now().timestamp_micros();

    #[cfg(test)]
    let timestamp_micros = 0;

    // switch kind
    match bytes.get(0)? & 0x8 {
        // Digital Switch
        0 => {
            if bytes.len() == 1 {
                Some(SwitchInfo {
                    kind: super::switch::SwitchKind::Digital,
                    pin: (bytes[0] & 0b01111110) >> 1,
                    state: (bytes[0] & 1) as u16,
                    timestamp_micros,
                })
            } else {
                None
            }
        }
        // Analog Switch
        1 => {
            if bytes.len() == 2 {
                Some(SwitchInfo {
                    kind: super::switch::SwitchKind::Analog,
                    pin: (bytes[0] & 0b01111100) >> 2,
                    state: ((bytes[0] as u16 & 0b11) << 8) | bytes[1] as u16,
                    timestamp_micros,
                })
            } else {
                None
            }
        }
        _ => None,
    }

    // Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec() {
        // dec_cobs test
        assert_eq!(dec_cobs(vec![01, 01, 00]), Some(vec![00]));
        assert_eq!(dec_cobs(vec![01, 01, 01, 00]), Some(vec![00, 00]));
        assert_eq!(dec_cobs(vec![01, 02, 11, 01, 00]), Some(vec![00, 11, 00]));
        assert_eq!(
            dec_cobs(vec![03, 11, 22, 02, 33, 00]),
            Some(vec![11, 22, 00, 33])
        );

        // raw_to_switch_info test
        // FIXME: timestampは除かないといけない
        assert_eq!(
            raw_to_switch_info(vec![0b00000000]),
            Some(SwitchInfo {
                ..Default::default()
            })
        );
        assert_eq!(
            raw_to_switch_info(vec![0b00000011]),
            Some(SwitchInfo {
                pin: 1,
                state: 1,
                ..Default::default()
            })
        );
    }
}
