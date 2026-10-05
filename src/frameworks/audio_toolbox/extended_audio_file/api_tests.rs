/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Frame counts and end-of-file reads in the shared headless environment.
use super::*;
use crate::frameworks::audio_toolbox::audio_file::guest_audio_file_read_from_vec;
use crate::frameworks::audio_toolbox::audio_unit::AudioBuffer;

pub(crate) fn check_reads(env: &mut Environment) {
    for channels in [1u16, 2] {
        let samples: Vec<i16> = (0..5 * channels).map(|i| i as i16 * 97 - 300).collect();
        let mut wav = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(
                &mut wav,
                hound::WavSpec {
                    channels,
                    sample_rate: 22050,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for &sample in &samples {
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();
        }
        let file = guest_audio_file_read_from_vec(env, wav.into_inner()).unwrap();
        let reference = env.mem.alloc_and_write(OpaqueExtAudioFile { _filler: 0 });
        State::get(&mut env.framework_state)
            .extended_audio_files
            .insert(
                reference,
                ExtAudioFileHostObject {
                    guest_audio_file: file,
                    client_data_format: None,
                    current_bytes_read: 0,
                },
            );
        let size = env.mem.alloc_and_write(8u32);
        let length = env.mem.alloc_and_write(-1i64);
        assert_eq!(
            ExtAudioFileGetProperty(env, reference, fourcc(b"#frm"), size, length.cast()),
            0
        );
        assert_eq!(env.mem.read(length), 5);
        env.mem.write(size, 4);
        env.mem.write(length, -1);
        assert_eq!(
            ExtAudioFileGetProperty(env, reference, fourcc(b"#frm"), size, length.cast()),
            kAudioFileBadPropertySizeError
        );
        assert_eq!(env.mem.read(length), -1);
        let description = AudioStreamBasicDescription::from_audio_description(
            env.framework_state.audio_toolbox.audio_file.audio_files[&file]
                .audio_file
                .audio_description(),
        );
        let description = env.mem.alloc_and_write(description);
        assert_eq!(
            ExtAudioFileSetProperty(
                env,
                reference,
                kExtAudioFileProperty_ClientDataFormat,
                guest_size_of::<AudioStreamBasicDescription>(),
                description.cast().cast_const()
            ),
            0
        );
        let bytes_per_frame = 2 * u32::from(channels);
        let data = env.mem.alloc(10 * bytes_per_frame);
        let buffers = env.mem.alloc_and_write(AudioBufferList {
            number_buffers: 1,
            buffers: [AudioBuffer {
                number_channels: channels.into(),
                data_byte_size: 10 * bytes_per_frame,
                data,
            }],
        });
        let frames = env.mem.alloc_and_write(2u32);
        assert_eq!(ExtAudioFileRead(env, reference, frames, buffers), 0);
        assert_eq!(env.mem.read(frames), 2);
        env.mem.write(size, 8);
        assert_eq!(
            ExtAudioFileGetProperty(env, reference, fourcc(b"#frm"), size, length.cast()),
            0
        );
        assert_eq!(env.mem.read(length), 5); // Independent of read position.
        env.mem.write(frames, 10);
        assert_eq!(ExtAudioFileRead(env, reference, frames, buffers), 0);
        assert_eq!(env.mem.read(frames), 3); // Preserve the partial final read.
        let byte_size = env.mem.read(buffers).buffers[0].data_byte_size;
        assert_eq!(byte_size, 3 * bytes_per_frame);
        let expected: Vec<u8> = samples[2 * channels as usize..]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert_eq!(env.mem.bytes_at(data.cast(), byte_size), expected);
        env.mem.write(frames, 10);
        assert_eq!(ExtAudioFileRead(env, reference, frames, buffers), 0);
        assert_eq!(env.mem.read(frames), 0);
        let byte_size = env.mem.read(buffers).buffers[0].data_byte_size;
        assert_eq!(byte_size, 0);
        assert_eq!(ExtAudioFileDispose(env, reference), 0);
        assert!(!State::get(&mut env.framework_state)
            .extended_audio_files
            .contains_key(&reference));
        assert!(!env
            .framework_state
            .audio_toolbox
            .audio_file
            .audio_files
            .contains_key(&file));
        for ptr in [
            size.cast(),
            length.cast(),
            description.cast(),
            data,
            buffers.cast(),
            frames.cast(),
        ] {
            env.mem.free(ptr);
        }
    }
}

#[test]
#[ignore = "requires Rogue Planet 1.2.1 app in TOUCHHLE_ROGUE_GAME"]
fn rogue_planet_sound_loader() {
    use crate::abi::{CallFromHost, GuestFunction};
    use crate::objc::{id, msg_class, release};
    use crate::options::Options;
    let path = std::path::PathBuf::from(std::env::var_os("TOUCHHLE_ROGUE_GAME").unwrap());
    let mut fixtures: Vec<_> = std::fs::read_dir(&path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
        .map(|path| {
            let mut reader = hound::WavReader::open(&path).unwrap();
            let spec = reader.spec();
            let bytes: Vec<u8> = reader
                .samples::<i16>()
                .flat_map(|sample| sample.unwrap().to_le_bytes())
                .collect();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                spec,
                bytes,
            )
        })
        .collect();
    fixtures.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(fixtures.len(), 153);
    let data = crate::fs::BundleData::open_any(&path).unwrap();
    let (bundle, fs) = crate::bundle::Bundle::new_bundle_and_fs_from_host_path(data, true).unwrap();
    assert_eq!(bundle.bundle_identifier(), "com.gameloft.rogueplanet");
    let options = Options {
        headless: true,
        direct_memory_access: false,
        ..Default::default()
    };
    let mut env = Environment::new(bundle, fs, options, Vec::new()).unwrap();
    env.cpu.regs_mut()[crate::cpu::Cpu::SP] = 0xfffff000;
    let mut coroutine = corosensei::Coroutine::new(move |yielder, mut env: Environment| {
        env.with_yielder(yielder, move |env| {
            let pool: id = msg_class![env; NSAutoreleasePool new];
            let count = env.mem.alloc_and_write(0u32);
            let format = env.mem.alloc_and_write(0u32);
            let rate = env.mem.alloc_and_write(0i32);
            // Original ARM loader reached by the reported startup backtrace.
            let loader = GuestFunction::from_addr_and_thumb_flag(0x84da4, false);
            for (name, spec, expected) in &fixtures {
                let guest_path = env.bundle.bundle_path().join(name);
                let string = crate::frameworks::foundation::ns_string::from_rust_string(
                    env,
                    guest_path.as_str().into(),
                );
                let url: id = msg_class![env; NSURL fileURLWithPath:string];
                let pcm: MutVoidPtr =
                    loader.call_from_host(env, (url, count, format, rate, MutPtr::<f32>::null()));
                assert!(!pcm.is_null(), "{name}");
                let size = env.mem.read(count);
                assert_eq!(size as usize, expected.len(), "{name}");
                assert_eq!(
                    env.mem.read(format),
                    if spec.channels == 1 { 0x1101 } else { 0x1103 }
                );
                assert_eq!(env.mem.read(rate), spec.sample_rate as i32);
                assert_eq!(env.mem.bytes_at(pcm.cast(), size), expected, "{name}");
                env.mem.free(pcm);
                release(env, string);
            }
            for ptr in [count.cast(), format.cast(), rate.cast()] {
                env.mem.free(ptr);
            }
            crate::frameworks::foundation::ns_keyed_archiver::api_tests::check_rogue_layout(env);
            release(env, pool);
            assert!(State::get(&mut env.framework_state)
                .extended_audio_files
                .is_empty());
            assert!(env
                .framework_state
                .audio_toolbox
                .audio_file
                .audio_files
                .is_empty());
            println!("Rogue Planet original ARM loader: all 153 WAV files match");
        });
        env
    });
    while let corosensei::CoroutineResult::Yield(next) = coroutine.resume(env) {
        env = next;
    }
}
