// The WASI Preview 1 imports (`wasi_snapshot_preview1`) that the module's C and C++ code, Luau and
// WASI libc, links against. The page's import map resolves the bare specifier wasm-bindgen emits
// to this file.
//
// There's no file system or environment: standard output and error go to the console, a line at
// a time, clocks read `performance.now()` and `Date.now()`, and randomness comes from
// `crypto.getRandomValues`. Everything else fails with ENOSYS.

const ESUCCESS = 0;
const EBADF = 8;
const EINVAL = 28;
const ENOSYS = 52;
const ESPIPE = 70;

const CLOCK_REALTIME = 0;
const FILETYPE_CHARACTER_DEVICE = 2;

let memory = null;

// Gives the shim the module's memory. It must come before the module first calls an import, so
// `captureMemory` takes it as the module is instantiated, before wasm-bindgen starts it.
export function setMemory(mem) {
  memory = mem;
}

// Wraps `WebAssembly.instantiate` and `instantiateStreaming` so the memory of the next module
// that imports this shim is handed to it as soon as it exists.
export function captureMemory() {
  const take = (result) => {
    const instance = result.instance ?? result;
    if (instance.exports?.memory) {
      setMemory(instance.exports.memory);
    }
    return result;
  };
  const instantiate = WebAssembly.instantiate;
  WebAssembly.instantiate = (source, imports, ...rest) =>
    instantiate.call(WebAssembly, source, imports, ...rest).then((result) =>
      imports?.wasi_snapshot_preview1 ? take(result) : result,
    );
  const streaming = WebAssembly.instantiateStreaming;
  if (streaming) {
    WebAssembly.instantiateStreaming = (source, imports, ...rest) =>
      streaming.call(WebAssembly, source, imports, ...rest).then((result) =>
        imports?.wasi_snapshot_preview1 ? take(result) : result,
      );
  }
}

const view = () => new DataView(memory.buffer);
const bytes = (ptr, len) => new Uint8Array(memory.buffer, ptr, len);

const decoder = new TextDecoder();
// Output not yet ended by a newline, for standard output and error.
const pending = { 1: "", 2: "" };

function flush(fd, text) {
  const lines = (pending[fd] + text).split("\n");
  pending[fd] = lines.pop();
  for (const line of lines) {
    (fd === 2 ? console.error : console.log)(line);
  }
}

export function fd_write(fd, iovs, iovsLen, nwritten) {
  if (fd !== 1 && fd !== 2) {
    return EBADF;
  }
  const v = view();
  let total = 0;
  let text = "";
  for (let i = 0; i < iovsLen; i++) {
    const ptr = v.getUint32(iovs + i * 8, true);
    const len = v.getUint32(iovs + i * 8 + 4, true);
    text += decoder.decode(bytes(ptr, len), { stream: true });
    total += len;
  }
  flush(fd, text);
  v.setUint32(nwritten, total, true);
  return ESUCCESS;
}

export function fd_fdstat_get(fd, stat) {
  if (fd > 2) {
    return EBADF;
  }
  const v = view();
  // A character device, as a terminal is, without the rights to seek, so libc buffers lines.
  v.setUint8(stat, FILETYPE_CHARACTER_DEVICE);
  v.setUint16(stat + 2, 0, true);
  v.setBigUint64(stat + 8, 0x40n, true); // FD_WRITE
  v.setBigUint64(stat + 16, 0n, true);
  return ESUCCESS;
}

export function fd_seek(fd) {
  return fd <= 2 ? ESPIPE : EBADF;
}

export function fd_close(fd) {
  return fd <= 2 ? ESUCCESS : EBADF;
}

export function fd_prestat_get() {
  return EBADF; // No preopened directories.
}

function sizes(count, size) {
  const v = view();
  v.setUint32(count, 0, true);
  v.setUint32(size, 0, true);
  return ESUCCESS;
}

export const environ_sizes_get = sizes;
export const args_sizes_get = sizes;
export function environ_get() {
  return ESUCCESS;
}
export function args_get() {
  return ESUCCESS;
}

export function clock_res_get(id, resolution) {
  view().setBigUint64(resolution, 1000n, true);
  return ESUCCESS;
}

export function clock_time_get(id, precision, time) {
  const ms = id === CLOCK_REALTIME ? Date.now() : performance.now();
  view().setBigUint64(time, BigInt(Math.round(ms * 1e6)), true);
  return ESUCCESS;
}

export function random_get(buf, len) {
  // getRandomValues takes at most 65536 bytes at a time.
  for (let i = 0; i < len; i += 65536) {
    crypto.getRandomValues(bytes(buf + i, Math.min(65536, len - i)));
  }
  return ESUCCESS;
}

export function sched_yield() {
  return ESUCCESS;
}

export function poll_oneoff() {
  return EINVAL;
}

export class ProcExit extends Error {
  constructor(code) {
    super(`the module exited with code ${code}`);
    this.code = code;
  }
}

export function proc_exit(code) {
  throw new ProcExit(code);
}

export function proc_raise() {
  return ENOSYS;
}

const unsupported = () => ENOSYS;
export {
  unsupported as fd_advise,
  unsupported as fd_allocate,
  unsupported as fd_datasync,
  unsupported as fd_fdstat_set_flags,
  unsupported as fd_fdstat_set_rights,
  unsupported as fd_filestat_get,
  unsupported as fd_filestat_set_size,
  unsupported as fd_filestat_set_times,
  unsupported as fd_pread,
  unsupported as fd_prestat_dir_name,
  unsupported as fd_pwrite,
  unsupported as fd_read,
  unsupported as fd_readdir,
  unsupported as fd_renumber,
  unsupported as fd_sync,
  unsupported as fd_tell,
  unsupported as path_create_directory,
  unsupported as path_filestat_get,
  unsupported as path_filestat_set_times,
  unsupported as path_link,
  unsupported as path_open,
  unsupported as path_readlink,
  unsupported as path_remove_directory,
  unsupported as path_rename,
  unsupported as path_symlink,
  unsupported as path_unlink_file,
  unsupported as sock_accept,
  unsupported as sock_recv,
  unsupported as sock_send,
  unsupported as sock_shutdown,
};
