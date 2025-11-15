+++
title = "Fun with #![no_std]"
date = 2025-11-14
+++

Many years ago (okay, two) I wrote my first program in Rust. The program is exceedingly simple - it prints, to stdout, a programming-related aphorism, in the style of UNIX [fortune](https://en.wikipedia.org/wiki/Fortune_(Unix)). Most of these aphorisms were taken from a webpage of advice supplied to me by Steve Hodges, the head of the computer science department at Cabrillo College. The program looks like this:

```rust
mod advice;

use advice::ADVICE;


fn main() {
    let rng: usize = fastrand::usize(..ADVICE.len());

    println!("{}", ADVICE[rng]);
}
```

where ADVICE is a static array of aphorisms, like

```rust
pub const ADVICE: &[&str] = &[
    "Write clearly - don't be too clever.",
    "Say what you mean, simply and directly.",
    ...

```

Not the most complex code ever written, I agree. And at 6 lines of code, quite small too. But I recently read Gabriel Dechichi's essay on [the hidden cost of software libraries](https://posts.cgamedev.com/p/the-hidden-cost-of-software-libraries) and it got me thinking. I wonder how small this binary is? First, let's look at the size of the source code:

```console
$ du -h src/main.rs
4.0K    src/main.rs
$ du -h src/advice.rs
4.0K    src/advice.rs
```

Only 8KB! But what about the binary?

```console
$ hyperfine --prepare 'cargo clean' --runs 100  'cargo build --release'
Benchmark 1: cargo build --release
  Time (mean ± σ):     459.9 ms ±  20.1 ms    [User: 312.0 ms, System: 155.9 ms]
  Range (min … max):   419.8 ms … 511.5 ms    100 runs
$ du -h target/release/adv
460K    target/release/adv
```

A ~460ms build time (not bad), but the file size shows a 60x (okay, 57.5x) increase! Wow!! Well, it's still only half a megabyte though...

"Hold on, you idiot!" I hear you interject "You don't need an external dependency to get a pseudorandom number! Just read from /dev/urandom!" You're right! Let's rewrite this with \~no external dependencies\~

```rust

use std::fs::File;
use std::io::Read;

fn main() {
  let mut buf = [0u8; 8];

  File::open("/dev/urandom")
    .expect("Failed to open /dev/urandom")
    .read_exact(&mut buf)
    .expect("Failed to read from /dev/urandom");

  let idx = usize::from_ne_bytes(buf) % ADVICE.len();
  println!("{}", ADVICE[idx]);
}
```

That should make it smaller, right?

```console
$ du -h target/release/adv
456K    target/release/adv
```

...well, technically it is smaller! But it looks like including std::fs and std::io means we don't get much of a filesize optimization at all. So, how can we get the smallest possible binary in Rust?

## Enter `#![no_std]`

For the uninitiated, `#![no_std]` is a directive that tells the compiler to exclude the standard library entirely from your program. When you use `no_std`, you only get access to [core](https://doc.rust-lang.org/core), the minimal, platform-agnostic library providing basic types, traits, and functions. It's intended for use in embedded systems, bootloaders, and kernels where there's no OS to provide these services. But, to save a few bytes, these are the depths we must plunge.

### 1. System Calls

Without std, I can't use `println!` or `std::fs`. I needed to talk directly to the Linux kernel using the [x86_64](https://en.wikipedia.org/wiki/X86-64) system call interface. For the uninitiated, a syscall is a way for a humble programmer to kneel at the altar of the almighty kernel and receive blessings. Thankfully, Rust's `core` library makes this easy to do.

An x86_64 syscall works by placing values in specific [CPU registers](https://en.wikipedia.org/wiki/Processor_register):

- `rax` contains the syscall number - an identifier telling the kernel what operation you want (1 = write, 318 = getrandom, etc.) You can find a complete list in the [manual](https://man7.org/linux/man-pages/man2/syscalls.2.html)
- `rdi`, `rsi`, and `rdx` contain arguments to the syscall such as a file descriptor, buffer, etc.

After the syscall executes, the `rax` register contains the return value.

```rust
unsafe fn syscall(n: usize, arg1: usize, arg2: usize, arg3: usize) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") n,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}
```

With this primitive, we can build higher-level operations:

```rust
unsafe fn write(fd: usize, buf: &[u8]) -> usize {
    syscall(1, fd, buf.as_ptr() as usize, buf.len())
}

unsafe fn getrandom(buf: &mut [u8]) -> usize {
    syscall(318, buf.as_mut_ptr() as usize, buf.len(), 0)
}

unsafe fn exit(code: usize) -> ! {
  syscall(60, code, 0, 0);
  loop {}
}
```

### 2. Doing Nothing Forever

When you neglect to include the standard library, you lose Rust's panic runtime - the code that prints a nice error message and backtrace when something goes wrong. In a `no_std` environment, you must provide your own panic handler.

The panic handler has one job: decide what to do when the program panics. For us, the answer is simple: We will do nothing, forever.

```rust
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
  loop {}
}
```

The function signature uses `-> !` (the "never" type) because a panic handler must never return - it either loops forever, exits the process, or does something equally terminal. You might wonder "Why not just print an error message?" Well, because the state of the system is undefined, executing a syscall risks making things worse. The convention in a `no_std` environment is to simply loop forever and allow the unlucky user to deal with the mess.

### 3. The Easy Stuff

The next challenge: `fn main()` depends on the standard library runtime. Under normal circumstances, the standard library provides the `_start` function and calls your `main()`. But in our case, we will use that for our main code:

```rust
#![no_std]
#![no_main]

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    unsafe {
        let mut buf = [0u8; 8];
        getrandom(&mut buf);

        let idx = usize::from_ne_bytes(buf) % ADVICE.len();
        write(1, ADVICE[idx].as_bytes());
        write(1, b"\n");

        exit(0);
    }
}
```

### 4. Missing Pieces

The linker complained about two missing symbols:

**`memset`**: The compiler generates calls to `memset` when initializing arrays. Since we are not linking against libc, I have to provide my own implementation:

```rust
#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *s.add(i) = c as u8;
        i += 1;
    }
    s
}
```

**`rust_eh_personality`**: Needed for exception handling metadata, even though I'm using `panic = "abort"`. An empty stub satisfies the linker:

```rust
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}
```

### 5. Linker Configuration

Finally, I needed to tell the linker not to include the standard C runtime. In `.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-nostartfiles"]
```

And in `Cargo.toml`:

```toml
[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

## The Result

```console
$ hyperfine --prepare 'cargo clean' --runs 100  'cargo build --release'
Benchmark 1: cargo build --release
  Time (mean ± σ):     183.5 ms ±  18.0 ms    [User: 88.7 ms, System: 96.6 ms]
  Range (min … max):   153.3 ms … 246.9 ms    100 runs
$ du -h target/release/adv
8.0K    target/release/adv
```

We cut the build time down by 200ms! But more importantly, we got the file size down to just 8KB - a 98% reduction from the original 460KB! Incredibly tiny!

The final program is remarkably simple. Get random bytes, pick an index, write to stdout, exit. No layers of abstraction, no hidden costs.

### Was It Worth It?

For a production system? Probably not. The `no_std` version is:

- **Platform-specific** (x86_64 Linux only)
- **Harder to maintain** (raw syscalls are less readable)
- **Missing safety guarantees** (lots of `unsafe`)

But the exercise was fun! I learned:

- What the standard library actually provides
- How system calls work at the assembly level
- The real cost of convenience abstractions

The standard library version compiles to 460KB not because Rust is bloated, but because it includes panic handling, formatting, UTF-8 validation, and cross-platform abstractions. For most programs, that's a reasonable trade-off. Of course, if you wanted to make this really small, you could rewrite it in assembly... but that's a task for another time.

If you'd like to see this for yourself, you can see both the [regular](https://github.com/grfwings/adv/tree/master) and [no_std](https://github.com/grfwings/adv/tree/nostd) on my [github](https://github.com/grfwings). Additionally, this program is available in the [AUR](https://aur.archlinux.org/packages/adv) if you'd like to try it yourself.
