// WASAPI capture only: no knowledge of the video encoder or the feeder thread. Each capture
// thread owns one endpoint (system loopback on the default render device, or the microphone on
// the default or a chosen capture device), converts every packet to the fixed target format and
// pushes it onto a shared queue. A device that disappears mid-recording
// (AUDCLNT_E_DEVICE_INVALIDATED) ends only that thread and marks the queue "lost"; the
// recording and the other audio source are unaffected.
use super::{AudioQueueShared, AudioResampler, REC_AUDIO_CHANNELS, REC_AUDIO_SAMPLE_RATE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, EDataFlow, ERole, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE, WAVEFORMATEXTENSIBLE,
};

// WAVE_FORMAT_IEEE_FLOAT / WAVE_FORMAT_EXTENSIBLE (mmreg.h) and the
// KSDATAFORMAT_SUBTYPE_IEEE_FLOAT GUID (ksmedia.h): not exposed by the windows crate's enabled
// features, so defined from their documented values.
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00AA00389B71);
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::core::HRESULT;

// AUDCLNT_E_DEVICE_INVALIDATED, from mmreg/audioclient.h: not exposed as
// a named constant by the windows crate's Win32_Media_Audio feature.
const AUDCLNT_E_DEVICE_INVALIDATED: HRESULT = HRESULT(0x88890004u32 as i32);

pub struct MicDevice {
    pub id: String,
    pub name: String,
}

// Enumerates active capture (microphone) endpoints for the settings dropdown. Best-effort: a
// failure yields an empty list, so a broken audio stack falls back to the default device.
pub fn list_mic_devices() -> Vec<MicDevice> {
    let mut out = Vec::new();
    // S_OK and S_FALSE both add a reference that this call must give back; a thread already in
    // another apartment mode (RPC_E_CHANGED_MODE) added none.
    let com_ref = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
    list_mic_devices_into(&mut out);
    if com_ref {
        unsafe { CoUninitialize() };
    }
    out
}

// The device Windows speech recognition records from: the default capture endpoint. None when
// there is no microphone or the audio stack fails.
pub fn default_mic_name() -> Option<String> {
    default_mic_name_for(eConsole)
}

// The default capture endpoint of one role (console or communications): Windows keeps a separate
// default per role and the two can name different devices.
pub fn default_mic_name_for(role: ERole) -> Option<String> {
    let com_ref = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
    let name = default_mic_name_inner(role);
    if com_ref {
        unsafe { CoUninitialize() };
    }
    name
}

// Kept apart so every COM object is dropped before the caller's CoUninitialize.
fn default_mic_name_inner(role: ERole) -> Option<String> {
    unsafe {
        let enumerator = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, role).ok()?;
        device_friendly_name(&device)
    }
}

// Kept apart so every COM object is dropped before the caller's CoUninitialize.
fn list_mic_devices_into(out: &mut Vec<MicDevice>) {
    unsafe {
        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return;
        };
        let Ok(collection): windows::core::Result<IMMDeviceCollection> =
            enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
        else {
            return;
        };
        let Ok(count) = collection.GetCount() else { return };
        for i in 0..count {
            let Ok(device) = collection.Item(i) else { continue };
            let Ok(id_ptr) = device.GetId() else { continue };
            let id = id_ptr.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(id_ptr.0 as *const _));
            let name = device_friendly_name(&device).unwrap_or_else(|| id.clone());
            out.push(MicDevice { id, name });
        }
    }
}

unsafe fn device_friendly_name(device: &IMMDevice) -> Option<String> {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    let store = device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ).ok()?;
    let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
    let s = windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc(&value).ok()?;
    let name = s.to_string().unwrap_or_default();
    windows::Win32::System::Com::CoTaskMemFree(Some(s.0 as *const _));
    Some(name)
}

// Whether a captured packet adds to the timeline. A packet flagged silent may come with no data
// pointer at all, and dropping it would cut that stretch of silence out of the track and shift
// everything after it.
// Packed little-endian 24-bit samples, as some USB and studio devices deliver them.
pub(crate) fn int24_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<3>()
        .0
        .iter()
        .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_607.0)
        .collect()
}

// Unsigned 8-bit samples, silence at 128.
pub(crate) fn uint8_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.iter().map(|&b| (f32::from(b) - 128.0) / 127.0).collect()
}

pub(crate) fn packet_has_audio(has_data: bool, num_frames: u32, silent: bool) -> bool {
    num_frames > 0 && (silent || has_data)
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
// set or the device is invalidated. A refused thread spawn is the caller's start error, not a panic.
pub fn spawn_capture(
    label: &'static str,
    flow: EDataFlow,
    loopback: bool,
    device_id: String,
    queue: Arc<AudioQueueShared>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name(format!("rec-audio-{label}"))
        .spawn(move || unsafe {
            let com_ref = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = (|| -> windows::core::Result<()> {
                let device = open_endpoint(flow, &device_id)?;
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
                let wave_format = client.GetMixFormat()?;
                let in_channels = (*wave_format).nChannels as u32;
                let in_rate = (*wave_format).nSamplesPerSec;
                let in_bits = (*wave_format).wBitsPerSample;
                // GetMixFormat's shared-mode result is WAVEFORMATEXTENSIBLE in practice, and a 32-bit stream
                // is not always IEEE float: some drivers report 32-bit integer PCM, which decoded as float
                // would be full-scale noise. With WAVE_FORMAT_EXTENSIBLE the real sample type is in
                // Format.SubFormat.
                let in_is_float = if (*wave_format).wFormatTag == WAVE_FORMAT_EXTENSIBLE {
                    let ext = wave_format as *const WAVEFORMATEXTENSIBLE;
                    std::ptr::addr_of!((*ext).SubFormat).read_unaligned() == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
                } else {
                    (*wave_format).wFormatTag == WAVE_FORMAT_IEEE_FLOAT
                };
                let stream_flags = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
                // The mix format is a CoTaskMem block the caller owns; Initialize
                // copies what it needs, so free it right after, success or not.
                let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, stream_flags, 2_000_000, 0, wave_format, None);
                windows::Win32::System::Com::CoTaskMemFree(Some(wave_format as *const _));
                init?;
                let capture_client: IAudioCaptureClient = client.GetService()?;
                client.Start()?;
                let mut resampler = AudioResampler::new(in_channels, in_rate, REC_AUDIO_CHANNELS, REC_AUDIO_SAMPLE_RATE);
                while !stop.load(Ordering::Relaxed) {
                    let mut packet_len = capture_client.GetNextPacketSize()?;
                    while packet_len != 0 {
                        let mut data_ptr = std::ptr::null_mut();
                        let mut num_frames = 0u32;
                        let mut flags = 0u32;
                        capture_client.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)?;
                        let is_silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                        if packet_has_audio(!data_ptr.is_null(), num_frames, is_silent) {
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
                            } else if in_bits == 24 {
                                int24_to_f32(std::slice::from_raw_parts(data_ptr, n * 3))
                            } else if in_bits == 8 {
                                uint8_to_f32(std::slice::from_raw_parts(data_ptr, n))
                            } else {
                                vec![0.0; n] // unsupported native format -- silence rather than garbage
                            };
                            let pcm = resampler.convert(&floats);
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
                if e.code() != AUDCLNT_E_DEVICE_INVALIDATED {
                    super::append_crash_log(&format!("recording audio ({label}): {e}"));
                }
                // Any failure ends this source's capture, not only an unplugged device (a missing device or
                // a refused start), and must reach the bar instead of leaving a silent track.
                queue.mark_lost();
            }
            if com_ref {
                CoUninitialize();
            }
        })
}
