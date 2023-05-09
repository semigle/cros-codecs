// Copyright 2022 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::io::Cursor;
use std::io::Seek;

use bytes::Buf;

use crate::decoders::h264::parser::Nalu;
use crate::decoders::h264::parser::NaluType;
use crate::decoders::BlockingMode;
use crate::decoders::DecodeError;
use crate::decoders::DecodedHandle;
use crate::decoders::DecoderEvent;
use crate::decoders::VideoDecoder;
use crate::utils::nalu::Header;

#[cfg(test)]
pub(crate) mod dummy;
pub mod nalu;
pub(crate) mod nalu_reader;
#[cfg(feature = "vaapi")]
pub mod vaapi;

/// Iterator over IVF packets.
pub struct IvfIterator<'a> {
    data: &'a [u8],
    cursor: Cursor<&'a [u8]>,
}

impl<'a> IvfIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut cursor = Cursor::new(data);

        // Skip the IVH header entirely.
        cursor.seek(std::io::SeekFrom::Start(32)).unwrap();

        Self { data, cursor }
    }
}

impl<'a> Iterator for IvfIterator<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        // Make sure we have a header.
        if self.cursor.remaining() < 6 {
            return None;
        }

        let len = self.cursor.get_u32_le() as usize;
        // Skip PTS.
        let _ = self.cursor.get_u64_le();

        if self.cursor.remaining() < len {
            return None;
        }

        let start = self.cursor.position() as usize;
        let _ = self.cursor.seek(std::io::SeekFrom::Current(len as i64));
        let end = self.cursor.position() as usize;

        Some(&self.data[start..end])
    }
}

/// A H.264 Access Unit.
#[derive(Debug, Default)]
pub struct AccessUnit<T> {
    pub nalus: Vec<Nalu<T>>,
}

/// A parser that produces Access Units from a list of NALUs. It does not use
/// section 7.4.1.2.4 of the specification for the detection of the first VCL
/// NAL unit of a primary coded picture and instead uses an heuristic from
/// GStreamer that works well enough for most streams.
#[derive(Debug, Default)]
pub struct AccessUnitParser<T> {
    picture_started: bool,
    nalus: Vec<Nalu<T>>,
}

impl<T: AsRef<[u8]>> AccessUnitParser<T> {
    /// Use GStreamer's gsth264parse's heuristic to break into access units.
    /// Only yields back an access unit if:
    /// We had previously established that a picture had started and an AUD is seen.
    /// We had previously established that a picture had started, but SEI|SPS|PPS is seen.
    /// We had previously established that a picture had started, and the
    /// current slice refers to the next picture.
    pub fn accumulate(&mut self, nalu: Nalu<T>) -> Option<AccessUnit<T>> {
        if matches!(nalu.header().nalu_type(), NaluType::AuDelimiter) && self.picture_started {
            self.picture_started = false;
            return Some(AccessUnit {
                nalus: self.nalus.drain(..).collect::<Vec<_>>(),
            });
        }

        self.nalus.push(nalu);

        if !self.picture_started {
            self.picture_started = matches!(
                self.nalus.last().unwrap().header().nalu_type(),
                NaluType::Slice
                    | NaluType::SliceDpa
                    | NaluType::SliceDpb
                    | NaluType::SliceDpc
                    | NaluType::SliceIdr
                    | NaluType::SliceExt
            );
        } else if matches!(
            self.nalus.last().unwrap().header().nalu_type(),
            NaluType::Sei | NaluType::Sps | NaluType::Pps
        ) {
            self.picture_started = false;
            return Some(AccessUnit {
                nalus: self.nalus.drain(..).collect::<Vec<_>>(),
            });
        } else if matches!(
            self.nalus.last().unwrap().header().nalu_type(),
            NaluType::Slice | NaluType::SliceDpa | NaluType::SliceIdr
        ) {
            let data = self.nalus.last().unwrap().data().as_ref();
            let header_bytes = self.nalus.last().unwrap().header().len();
            let mut r = nalu_reader::NaluReader::new(&data[header_bytes..]);

            let first_mb_in_slice = r.read_ue::<u32>();

            if first_mb_in_slice.is_ok() && first_mb_in_slice.unwrap() == 0 {
                self.picture_started = false;
                return Some(AccessUnit {
                    nalus: self.nalus.drain(..).collect::<Vec<_>>(),
                });
            }
        }

        None
    }
}

/// Iterator over groups of Nalus that can contain a whole frame.
pub struct H264FrameIterator<'a> {
    stream: &'a [u8],
    cursor: Cursor<&'a [u8]>,
    aud_parser: AccessUnitParser<&'a [u8]>,
}

impl<'a> H264FrameIterator<'a> {
    pub fn new(stream: &'a [u8]) -> Self {
        Self {
            stream,
            cursor: Cursor::new(stream),
            aud_parser: Default::default(),
        }
    }
}

impl<'a> Iterator for H264FrameIterator<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        while let Ok(Some(nalu)) = Nalu::next(&mut self.cursor) {
            if let Some(access_unit) = self.aud_parser.accumulate(nalu) {
                let start_nalu = access_unit.nalus.first().unwrap();
                let end_nalu = access_unit.nalus.last().unwrap();

                let start_offset = start_nalu.sc_offset();
                let end_offset = end_nalu.offset() + end_nalu.size();

                let data = &self.stream[start_offset..end_offset];

                return Some(data);
            }
        }

        // Process any left over NALUs, even if we could not fit them into an AU using the
        // heuristic.
        if !self.aud_parser.nalus.is_empty() {
            let nalus = self.aud_parser.nalus.drain(..).collect::<Vec<_>>();
            let start_nalu = nalus.first().unwrap();
            let end_nalu = nalus.last().unwrap();

            let start_offset = start_nalu.sc_offset();
            let end_offset = end_nalu.offset() + end_nalu.size();

            let data = &self.stream[start_offset..end_offset];

            Some(data)
        } else {
            None
        }
    }
}

/// Simple decoding loop that plays the stream once from start to finish.
pub fn simple_playback_loop<'a, D, I>(
    decoder: &mut D,
    stream_iter: I,
    on_new_frame: &mut dyn FnMut(Box<dyn DecodedHandle>),
    blocking_mode: BlockingMode,
) where
    D: VideoDecoder + ?Sized,
    I: Iterator<Item = &'a [u8]>,
{
    // Closure that drains all pending decoder events and calls `on_new_frame` on each
    // completed frame.
    let mut check_events = |decoder: &mut D| {
        while let Some(event) = decoder.next_event() {
            match event {
                DecoderEvent::FrameReady(frame) => {
                    on_new_frame(frame);
                }
                DecoderEvent::FormatChanged(_) => {}
            }
        }
    };

    for (frame_num, packet) in stream_iter.enumerate() {
        loop {
            match decoder.decode(frame_num as u64, packet) {
                Ok(()) => {
                    if blocking_mode == BlockingMode::Blocking {
                        check_events(decoder);
                    }
                    // Break the loop so we can process the next NAL if we sent the current one
                    // successfully.
                    break;
                }
                Err(DecodeError::CheckEvents) => check_events(decoder),
                Err(e) => panic!("{:#}", e),
            }
        }
    }

    decoder.flush();
    check_events(decoder);
}
