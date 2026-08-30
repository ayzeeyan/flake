# Flake Standard Library Reference

Flake provides a rich standard library covering core data structures, utilities, systems programming, and concurrency.

Modules are imported using `import <module_name>`.

---

## 1. Systems Programming Modules (v0.8+)

### `import fs`
Filesystem manipulation and I/O.

- `read_to_string(path: String) -> Result[String, String] / io + alloc`
  Reads the full contents of a file as a string.
- `write_string(path: String, contents: String) -> Result[Nil, String] / io`
  Writes string content to a file (creates or overwrites).
- `exists(path: String) -> Bool / io`
  Returns `true` if the file or directory exists.
- `remove(path: String) -> Result[Nil, String] / io`
  Deletes the specified file from disk.
- `file_size(path: String) -> Result[Int, String] / io + alloc`
  Returns the size of the file in bytes.
- `is_directory(path: String) -> Bool / io`
  True if `path` names a directory.
- `is_regular_file(path: String) -> Bool / io`
  True if `path` names a regular file.
- `read_dir(path: String) -> Result[[String], String] / io + alloc`
  Lists a directory's entries (sorted). Missing paths and files return `Err`.
- `walk(root: String) -> Result[[String], String] / io + alloc`
  Recursively lists `root` and all descendants. Missing paths return `Err`.
- `read_lines(path: String) -> Result[[String], String] / io + alloc`
  Reads a file and splits it on newlines.
- `write_lines(path: String, lines: [String]) -> Result[Nil, String] / io + alloc`
  Joins lines with newlines and writes the file.
- `append_string(path: String, contents: String) -> Result[Nil, String] / io`
  Appends to a file, creating it if needed.
- `create_directory(path: String) -> Result[Nil, String] / io`
  Creates a single directory (parents must already exist).

### `import path`
Cross-platform path manipulation and normalization.

- `join_path(a: String, b: String) -> String`
  Joins two path segments with normalized separators.
- `is_absolute(p: String) -> Bool`
  Checks if a path is absolute (supports Unix `/` and Windows `C:`/`\\`).
- `file_name(p: String) -> String`
  Extracts the file name and extension from a path.
- `parent(p: String) -> String`
  Extracts the parent directory path.
- `extension(p: String) -> String`
  Extracts the file extension without the leading dot.
- `normalize(p: String) -> String`
  Normalizes separators and resolves redundant `.` / `..` components.

### `import process`
Process environment and lifecycle utilities.

- `ProcessOutput { stdout: String, stderr: String, exit_code: Int }`
- `current_dir() -> Result[String, String] / io + alloc`
  Returns the current working directory.
- `env_var(name: String) -> Option[String] / io + alloc`
  Retrieves an environment variable value if set.
- `exit(code: Int) / panic`
  Terminates process execution with the specified exit code.
- `program_args() -> [String] / io + alloc`
  Arguments forwarded to the Flake program (`flake run file.flk -- a b`).
- `run(command: String) -> Result[ProcessOutput, String] / io + alloc`
  Runs a shell command and captures stdout, stderr, and exit code.
  Interpreter and VM are complete; native currently returns an empty capture.

### `import bytes`
Efficient byte buffer representations and manipulation.

- `ByteBuffer { raw: String, len: Int }`
- `new_buffer() -> ByteBuffer`
  Creates an empty byte buffer.
- `from_string(s: String) -> ByteBuffer`
  Initializes a byte buffer from a string.
- `append_byte(b: ByteBuffer, byte_val: Int) -> ByteBuffer`
  Appends an ASCII byte (0..255) to the buffer.
- `append_bytes(a: ByteBuffer, b: ByteBuffer) -> ByteBuffer`
  Concatenates two byte buffers.
- `get(b: ByteBuffer, idx: Int) -> Option[Int]`
  Returns the byte at index `idx`.
- `slice(b: ByteBuffer, start: Int, end: Int) -> ByteBuffer`
  Returns a sub-slice of the byte buffer.
- `len_bytes(b: ByteBuffer) -> Int`
  Returns the number of bytes in the buffer.

### `import channel`
Typed concurrent communication channels.

- `Channel[T] { buffer: List[T], capacity: Int, closed: Bool }`
- `new_channel[T](capacity: Int) -> Channel[T]`
  Creates a new typed channel with buffer capacity.
- `send[T](ch: Channel[T], item: T) -> Result[Channel[T], String]`
  Sends an item into the channel.
- `recv[T](ch: Channel[T]) -> Result[T, String]`
  Receives the next item from the channel.
- `try_recv[T](ch: Channel[T]) -> Option[T]`
  Non-blocking inspection of the channel buffer head.
- `close_channel[T](ch: Channel[T]) -> Channel[T]`
  Closes the channel.
- `is_closed[T](ch: Channel[T]) -> Bool`
- `is_empty[T](ch: Channel[T]) -> Bool`
- `is_full[T](ch: Channel[T]) -> Bool`
- `len_channel[T](ch: Channel[T]) -> Int`

---

Generic list helpers (v0.9): `sort_items[T: Ord]`, `find_eq[T: Eq]`,
`contains_eq[T: Eq]`, `max_ord[T: Ord]`, `min_ord[T: Ord]` live in
`import list`.

## 2. Core Modules

### `import option`
- `enum Option[T] { Some(T), None }`
- `is_some[T](opt: Option[T]) -> Bool`
- `is_none[T](opt: Option[T]) -> Bool`
- `unwrap_or[T](opt: Option[T], fallback: T) -> T`

### `import result`
- `enum Result[T, E] { Ok(T), Err(E) }`
- `is_ok[T, E](r: Result[T, E]) -> Bool`
- `is_err[T, E](r: Result[T, E]) -> Bool`
- `unwrap[T, E](r: Result[T, E]) -> T`
- `unwrap_err[T, E](r: Result[T, E]) -> E`

### `import math`
- `abs(x)`
- `min(a, b)`, `max(a, b)`
- `clamp(val, low, high)`
- `pow(base, exp)`

### `import string`
- `len(s)`
- `starts_with(s, prefix)`, `ends_with(s, suffix)`
- `trim(s)`, `upper(s)`, `lower(s)`
- `substring(s, start, end)`
- `split(s, sep)`, `join(list, sep)`
