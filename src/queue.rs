//! V4L2 OUTPUT / CAPTURE キューの管理。

use std::collections::VecDeque;
use std::os::fd::{AsRawFd, RawFd};

use crate::buffer::{BufferSet, PlaneMapping};
use crate::sys;

/// OUTPUT キュー (エンコーダー/デコーダーへの入力)。
pub(crate) struct OutputQueue {
    fd: RawFd,
    buf_type: u32,
    memory: u32,
    buffers: BufferSet,
    available: VecDeque<u32>,
}

impl OutputQueue {
    /// OUTPUT キューを作成する。
    pub fn new(fd: RawFd, buf_type: u32, memory: u32, buffers: BufferSet) -> Self {
        let count = buffers.count();
        let mut available = VecDeque::with_capacity(count as usize);
        for i in 0..count {
            available.push_back(i);
        }
        OutputQueue {
            fd,
            buf_type,
            memory,
            buffers,
            available,
        }
    }

    /// 利用可能なバッファインデックスを取得する。
    pub fn dequeue_available(&mut self) -> Option<u32> {
        self.available.pop_front()
    }

    /// バッファインデックスを利用可能リストに戻す。
    pub fn return_buffer(&mut self, index: u32) {
        self.available.push_back(index);
    }

    /// データをバッファにコピーして QBUF する。
    pub fn enqueue(
        &mut self,
        index: u32,
        data: &[u8],
        timestamp_us: i64,
    ) -> crate::error::Result<()> {
        // 先にバッファ情報を取得してボローを解放
        let plane_length = self.buffers.plane(index, 0).length;
        let is_dmabuf = matches!(
            &self.buffers.plane(index, 0).mapping,
            PlaneMapping::DmaBuf(_)
        );
        let dmabuf_fd = self.buffers.dmabuf_fd(index, 0);

        if data.len() > plane_length as usize {
            return Err(crate::error::Error::InputTooLarge {
                size: data.len(),
                capacity: plane_length as usize,
            });
        }

        // mmap バッファにデータをコピー
        if let Some(buf) = self.buffers.mmap_slice_mut(index, 0) {
            buf[..data.len()].copy_from_slice(data);
        }

        let timestamp = sys::timestamp_us_to_timeval(timestamp_us);

        let mut plane_info = sys::v4l2_plane {
            bytesused: data.len() as u32,
            length: plane_length,
            m: if is_dmabuf {
                sys::v4l2_plane_m {
                    fd: dmabuf_fd.unwrap_or(-1),
                }
            } else {
                sys::v4l2_plane_m { mem_offset: 0 }
            },
            data_offset: 0,
            reserved: [0; 11],
        };

        let mut buf = sys::zeroed_buffer(self.buf_type, self.memory);
        buf.index = index;
        buf.length = 1;
        buf.flags = sys::V4L2_BUF_FLAG_TIMESTAMP_COPY;
        buf.timestamp = timestamp;
        buf.m = sys::v4l2_buffer_m {
            planes: &mut plane_info as *mut _,
        };

        sys::ioctl_qbuf(self.fd, &mut buf)
    }

    /// DMABUF fd を設定して QBUF する (ゼロコピー)。
    pub fn enqueue_dmabuf(
        &mut self,
        index: u32,
        dmabuf_fd: RawFd,
        bytesused: u32,
        length: u32,
        timestamp_us: i64,
    ) -> crate::error::Result<()> {
        let timestamp = sys::timestamp_us_to_timeval(timestamp_us);

        let mut plane_info = sys::v4l2_plane {
            bytesused,
            length,
            m: sys::v4l2_plane_m { fd: dmabuf_fd },
            data_offset: 0,
            reserved: [0; 11],
        };

        let mut buf = sys::zeroed_buffer(self.buf_type, sys::V4L2_MEMORY_DMABUF);
        buf.index = index;
        buf.length = 1;
        buf.flags = sys::V4L2_BUF_FLAG_TIMESTAMP_COPY;
        buf.timestamp = timestamp;
        buf.m = sys::v4l2_buffer_m {
            planes: &mut plane_info as *mut _,
        };

        sys::ioctl_qbuf(self.fd, &mut buf)
    }
}

/// CAPTURE キュー (エンコーダー/デコーダーからの出力)。
pub(crate) struct CaptureQueue {
    fd: RawFd,
    buf_type: u32,
    memory: u32,
    buffers: BufferSet,
}

impl CaptureQueue {
    /// CAPTURE キューを作成する。
    pub fn new(fd: RawFd, buf_type: u32, memory: u32, buffers: BufferSet) -> Self {
        CaptureQueue {
            fd,
            buf_type,
            memory,
            buffers,
        }
    }

    /// 全バッファを QBUF する (初期化時に使用)。
    pub fn enqueue_all(&mut self) -> crate::error::Result<()> {
        for i in 0..self.buffers.count() {
            self.enqueue(i)?;
        }
        Ok(())
    }

    /// 指定インデックスのバッファを QBUF する。
    pub fn enqueue(&mut self, index: u32) -> crate::error::Result<()> {
        let plane = self.buffers.plane(index, 0);

        let mut plane_info = sys::v4l2_plane {
            bytesused: 0,
            length: plane.length,
            m: match &plane.mapping {
                PlaneMapping::Mmap(_) => sys::v4l2_plane_m { mem_offset: 0 },
                PlaneMapping::DmaBuf(fd) => sys::v4l2_plane_m { fd: fd.as_raw_fd() },
                PlaneMapping::None => unreachable!("CAPTURE バッファに NoMapping は使用されない"),
            },
            data_offset: 0,
            reserved: [0; 11],
        };

        let mut buf = sys::zeroed_buffer(self.buf_type, self.memory);
        buf.index = index;
        buf.length = 1;
        buf.m = sys::v4l2_buffer_m {
            planes: &mut plane_info as *mut _,
        };

        sys::ioctl_qbuf(self.fd, &mut buf)
    }

    /// バッファセットへの参照を取得する。
    pub fn buffers(&self) -> &BufferSet {
        &self.buffers
    }
}
