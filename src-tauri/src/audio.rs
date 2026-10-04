// WASAPI capture only: no knowledge of the video encoder or the feeder
// thread. Each capture thread owns exactly one endpoint (system loopback
// on the default render device, or the microphone on the default -- or a
// chosen -- capture device), converts every packet to the fixed target
// format and pushes it onto a shared queue. A device that disappears
// mid-recording (AUDCLNT_E_DEVICE_INVALIDATED) ends only that thread and
// marks the queue "lost" -- the recording itself, and the other audio
// source if any, are unaffected.
use super::{audio_convert_to_target, AudioQueueShared, REC_AUDIO_CHANNELS, REC_AUDIO_SAMPLE_RATE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, EDataFlow, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE, WAVEFORMATEXTENSIBLE,
};

// WAVE_FORMAT_IEEE_FLOAT / WAVE_FORMAT_EXTENSIBLE (mmreg.h) and the
// KSDATAFORMAT_SUBTYPE_IEEE_FLOAT GUID (ksmedia.h) -- like
// AUDCLNT_E_DEVICE_INVALIDATED below, not exposed by the windows crate's
// enabled features, so defined directly from their documented values.
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00AA00389B71);
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::core::HRESULT;

// AUDCLNT_E_DEVICE_INVALIDATED, from mmreg/audioclient.h -- not exposed as
// a named constant by the windows crate's Win32_Media_Audio feature.
const AUDCLNT_E_DEVICE_INVALIDATED: HRESULT = HRESULT(0x88890004u32 as i32);

pub struct MicDevice {
    pub id: String,
    pub name: String,
}

// Enumerates active capture (microphone) endpoints for the settings
// dropdown. Best-effort: any failure just yields an empty list, so a
// broken audio stack degrades to "use the default device" rather than
// failing the whole settings dialog.
pub fn list_mic_devices() -> Vec<MicDevice> {
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return out;
        };
        let Ok(collection): windows::core::Result<IMMDeviceCollection> =
            enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
        else {
            return out;
        };
        let Ok(count) = collection.GetCount() else { return out };
        for i in 0..count {
            let Ok(device) = collection.Item(i) else { continue };
            let Ok(id) = device.GetId() else { continue };
            let id = id.to_string().unwrap_or_default();
            let name = device_friendly_name(&device).unwrap_or_else(|| id.clone());
            out.push(MicDevice { id, name });
        }
    }
    out
}

unsafe fn device_friendly_name(device: &IMMDevice) -> Option<String> {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    let store = device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ).ok()?;
    let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
    let s = windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc(&value).ok()?;
    Some(s.to_string().unwrap_or_default())
}

fn open_endpoint(flow: EDataFlow, device_id: &str) -> windows::core::Result<IMMDevice> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        if device_id.is_empty() {
            enumerator.GetDefaultAudioEndpoint(flow, eConsole)
        } else {
            let id = windows::core::HSTRING::from(device_id);
            enumerator.GetDevice(&id)
        }
    }
}

// One capture thread. `loopback` selects system-audio capture on a render
// endpoint vs. plain capture on a capture endpoint; `device_id` picks a
// specific microphone (empty = the system default). Runs until `stop` is
// set or the device is invalidated.
pub fn spawn_capture(
    label: &'static str,
    flow: EDataFlow,
    loopback: bool,
    device_id: String,
    queue: Arc<AudioQueueShared>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(format!("rec-audio-{label}"))
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let result = (|| -> windows::core::Result<()> {
                let device = open_endpoint(flow, &device_id)?;
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
                let wave_format = client.GetMixFormat()?;
                let in_channels = (*wave_format).nChannels as u32;
                let in_rate = (*wave_format).nSamplesPerSec;
                let in_bits = (*wave_format).wBitsPerSample;
                // GetMixFormat's shared-mode result is WAVEFORMATEXTENSIBLE in
                // virtually every real case; a 32-bit-per-sample stream is not
                // always IEEE float -- some audio drivers report 32-bit
                // *integer* PCM, which decoded as float would be full-scale
                // noise. wFormatTag == WAVE_FORMAT_EXTENSIBLE means the real
                // sample type is in Format.SubFormat, not the outer tag.
                let in_is_float = if (*wave_format).wFormatTag == WAVE_FORMAT_EXTENSIBLE {
                    let ext = wave_format as *const WAVEFORMATEXTENSIBLE;
                    std::ptr::addr_of!((*ext).SubFormat).read_unaligned() == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
                } else {
                    (*wave_format).wFormatTag == WAVE_FORMAT_IEEE_FLOAT
                };
                let stream_flags = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
                client.Initialize(AUDCLNT_SHAREMODE_SHARED, stream_flags, 2_000_000, 0, wave_format, None)?;
                let capture_client: IAudioCaptureClient = client.GetService()?;
                client.Start()?;
                while !stop.load(Ordering::Relaxed) {
                    let mut packet_len = capture_client.GetNextPacketSize()?;
                    while packet_len != 0 {
                        let mut data_ptr = std::ptr::null_mut();
                        let mut num_frames = 0u32;
                        let mut flags = 0u32;
                        capture_client.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)?;
                        if !data_ptr.is_null() && num_frames > 0 {
                            let is_silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                            let n = (num_frames * in_channels) as usize;
                            let floats: Vec<f32> = if is_silent {
                                vec![0.0; n]
                            } else if in_bits == 32 && in_is_float {
                                std::slice::from_raw_parts(data_ptr as *const f32, n).to_vec()
                            } else if in_bits == 32 {
                                std::slice::from_raw_parts(data_ptr as *const i32, n)
                                    .iter()
                                    .map(|&s| s as f32 / i32::MAX as f32)
                                    .collect()
                            } else if in_bits == 16 {
                                std::slice::from_raw_parts(data_ptr as *const i16, n)
                                    .iter()
                                    .map(|&s| s as f32 / i16::MAX as f32)
                                    .collect()
                            } else {
                                vec![0.0; n] // unsupported native format -- silence rather than garbage
                            };
                            let pcm = audio_convert_to_target(
                                &floats, in_channels, in_rate, REC_AUDIO_CHANNELS, REC_AUDIO_SAMPLE_RATE,
                            );
                            queue.push(&pcm);
                        }
                        capture_client.ReleaseBuffer(num_frames)?;
                        packet_len = capture_client.GetNextPacketSize()?;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                client.Stop()?;
                Ok(())
            })();
            if let Err(e) = result {
                if e.code() == AUDCLNT_E_DEVICE_INVALIDATED {
                    queue.mark_lost();
                } else {
                    super::append_crash_log(&format!("recording audio ({label}): {e}"));
                }
            }
        })
        .expect("spawn rec-audio thread")
}
