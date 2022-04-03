#![feature(never_type)]

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tokio_serde::formats::SymmetricalBincode;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};
use tokio_util::compat::TokioAsyncReadCompatExt;

#[derive(Clone, Serialize, Deserialize)]
struct MyStruct {
    a: usize,
    b: Vec<u8>,
}

// This futures::io::AsyncRead adapter simulates a slow read source that returns 32 bytes
// at a time
const MAX_THROTTLED_READ: usize = 32;
struct ThrottledRead<W> {
    buf: Vec<u8>,
    pos: usize,
    len: usize,
    inner: W,
}
impl<W> ThrottledRead<W> {
	fn new(inner: W) -> Self {
        Self {
            buf: vec![0; 100*1024*1024], // big enough to handle whatever read we have
            pos: 0,
            len: 0,
            inner,
        }
	}
}
impl<W: futures::io::AsyncRead + Unpin> futures::io::AsyncRead for ThrottledRead<W> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let self_ = Pin::into_inner(self);
        // Need to read some more data?
		if self_.pos == self_.len {
            let n = futures::ready!(Pin::new(&mut self_.inner).poll_read(cx, &mut self_.buf)).unwrap();
            self_.pos = 0;
            self_.len = n;
        }

        // Got some data to copy, copy some of it
        let num_to_copy = cmp::min(MAX_THROTTLED_READ, self_.len-self_.pos);
        let num_to_copy = cmp::min(num_to_copy, buf.len());

        let elts = &self_.buf[self_.pos..self_.pos+num_to_copy];
        buf[..num_to_copy].copy_from_slice(elts);
        self_.pos += elts.len();
        Poll::Ready(Ok(elts.len()))
	}
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    //let rt = tokio::runtime::Runtime::new().unwrap();

    {
        let (src, dst) = futures_pipe_stream();
        let (src, dst) = framedcpy1(src, dst);
        let now = Instant::now();
        run(&rt, src, dst);
        println!("t1 {:?}", now.elapsed());
    }

    {
        let (src, dst) = futures_pipe_stream();
        let (src, dst) = framedcpy2(src, dst);
        let now = Instant::now();
        run(&rt, src, dst);
        println!("t2 {:?}", now.elapsed());
    }
}

fn run(
    rt: &tokio::runtime::Runtime,
    mut src: impl futures::Sink<MyStruct> + Unpin,
    mut dst: impl futures::Stream<Item=io::Result<MyStruct>> + Unpin,
) {
    let ms = MyStruct {
        a: 0,
        b: vec![55; 1024*2],
    };
    let fut = futures::future::join(
        async {
            for i in 0..1_000_000 {
                let mut x = ms.clone();
                x.a = i;
                let res = src.send(x).await;
                assert!(res.is_ok());
            }
        },
        async {
            for i in 0..1_000_000 {
                let x = dst.next().await.unwrap().unwrap();
                assert!(x.a == i);
            }
        },
    );
    rt.block_on(fut);
}

fn futures_pipe_stream()
        -> (impl futures::io::AsyncWrite, impl futures::io::AsyncRead) {
    let (src, dst) = tokio::io::duplex(4096);
    let inner_write = src.compat();
    let inner_read = ThrottledRead::new(dst.compat());
    (inner_write, inner_read)
}

fn framedcpy1(inner_write: impl futures::io::AsyncWrite, inner_read: impl futures::io::AsyncRead)
        -> (impl futures::Sink<MyStruct>, impl futures::Stream<Item=io::Result<MyStruct>>) {
    let inner_send = tokio_serde::Framed::<_, !, MyStruct, _>::new(
        FramedWrite::new(inner_write.compat_write(), LengthDelimitedCodec::new()),
        SymmetricalBincode::<MyStruct>::default(),
    );
    let inner_recv = tokio_serde::Framed::<_, MyStruct, !, _>::new(
        FramedRead::new(inner_read.compat(), LengthDelimitedCodec::new()), // <<<<<<<<<
        SymmetricalBincode::<MyStruct>::default(),
    );
    (inner_send, inner_recv)
}

const BUF_CAPACITY: usize = 10*1024*1024;
fn framedcpy2(inner_write: impl futures::io::AsyncWrite, inner_read: impl futures::io::AsyncRead)
        -> (impl futures::Sink<MyStruct>, impl futures::Stream<Item=io::Result<MyStruct>>) {
    let inner_send = tokio_serde::Framed::<_, !, MyStruct, _>::new(
        FramedWrite::new(inner_write.compat_write(), LengthDelimitedCodec::new()),
        SymmetricalBincode::<MyStruct>::default(),
    );
    let inner_recv = tokio_serde::Framed::<_, MyStruct, !, _>::new(
        FramedRead::new(tokio::io::BufReader::with_capacity(BUF_CAPACITY, inner_read.compat()), LengthDelimitedCodec::new()), // <<<<<<<<<
        SymmetricalBincode::<MyStruct>::default(),
    );
    (inner_send, inner_recv)
}
