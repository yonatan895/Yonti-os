# Yonti-os

You'll need to have access to rustup and cargo. QEMU is also needed. Currently for Linux x86_64 only.

Follow the steps below in order to run Yonti-os:

```
git clone https://github.com/yonatan895/Yonti-os

cd Yonti-os

rustup override set nightly

rustup component add rust-src

rustup component add llvm-tools-preview

cargo install bootimage

cargo run
```
