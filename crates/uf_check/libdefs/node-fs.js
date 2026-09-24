/**
 * @fileoverview Node's `fs` module, with `Dirent.name` typed as what it is.
 *
 * The vendored `evals/flow-typed/environment/node.js` declares
 * `Dirent.name: string | Buffer` for every call, although Node hands back a
 * `Buffer` only when the call asked for `encoding: "buffer"`. So
 * `readdirSync(dir, { withFileTypes: true }).map((entry) => entry.name)` was an
 * array of `string | Buffer`, and every consumer had to narrow a type Node
 * never produces. Nine errors in this repository came from it
 * (ubugeeei-prod/uf#1451).
 *
 * # How this replaces the vendored one
 *
 * A later `declare module` replaces an earlier one with the same name, so the
 * module is repeated **whole**: everything below is `node.js`'s, unchanged,
 * except that `Dirent` takes its name's type as a parameter (`string` by
 * default) and the `readdir` overloads with `withFileTypes` answer
 * `Dirent<Buffer>` for `encoding: "buffer"` and `Dirent<string>` otherwise.
 * `node:fs` and `fs/promises` are declared in terms of `$Exports<'fs'>`, so
 * they follow.
 */

declare module "fs" {
  declare class Stats {
    dev: number;
    ino: number;
    mode: number;
    nlink: number;
    uid: number;
    gid: number;
    rdev: number;
    size: number;
    blksize: number;
    blocks: number;
    atimeMs: number;
    mtimeMs: number;
    ctimeMs: number;
    birthtimeMs: number;
    atime: Date;
    mtime: Date;
    ctime: Date;
    birthtime: Date;

    isFile(): boolean;
    isDirectory(): boolean;
    isBlockDevice(): boolean;
    isCharacterDevice(): boolean;
    isSymbolicLink(): boolean;
    isFIFO(): boolean;
    isSocket(): boolean;
  }

  declare type PathLike = string | Buffer | URL;

  declare class FSWatcher extends events$EventEmitter {
    close(): void;
  }

  declare class ReadStream extends stream$Readable {
    close(): void;
  }

  declare class WriteStream extends stream$Writable {
    close(): void;
    bytesWritten: number;
  }

  // The entry's name: a string, unless the call that produced it asked for
  // `encoding: "buffer"`. The vendored class typed it `string | Buffer` for
  // every call, so no `readdir` with `withFileTypes` could read `.name` as the
  // string it is.
  declare class Dirent<out Name extends string | Buffer = string> {
    name: Name;

    isBlockDevice(): boolean;
    isCharacterDevice(): boolean;
    isDirectory(): boolean;
    isFIFO(): boolean;
    isFile(): boolean;
    isSocket(): boolean;
    isSymbolicLink(): boolean;
  }

  declare function rename(oldPath: string, newPath: string, callback?: (err: ?ErrnoError) => void): void;
  declare function renameSync(oldPath: string, newPath: string): void;
  declare function ftruncate(fd: number, len: number, callback?: (err: ?ErrnoError) => void): void;
  declare function ftruncateSync(fd: number, len: number): void;
  declare function truncate(path: string, len: number, callback?: (err: ?ErrnoError) => void): void;
  declare function truncateSync(path: string, len: number): void;
  declare function chown(path: string, uid: number, gid: number, callback?: (err: ?ErrnoError) => void): void;
  declare function chownSync(path: string, uid: number, gid: number): void;
  declare function fchown(fd: number, uid: number, gid: number, callback?: (err: ?ErrnoError) => void): void;
  declare function fchownSync(fd: number, uid: number, gid: number): void;
  declare function lchown(path: string, uid: number, gid: number, callback?: (err: ?ErrnoError) => void): void;
  declare function lchownSync(path: string, uid: number, gid: number): void;
  declare function chmod(path: string, mode: number | string, callback?: (err: ?ErrnoError) => void): void;
  declare function chmodSync(path: string, mode: number | string): void;
  declare function fchmod(fd: number, mode: number | string, callback?: (err: ?ErrnoError) => void): void;
  declare function fchmodSync(fd: number, mode: number | string): void;
  declare function lchmod(path: string, mode: number | string, callback?: (err: ?ErrnoError) => void): void;
  declare function lchmodSync(path: string, mode: number | string): void;
  declare function stat(path: string, callback?: (err: ?ErrnoError, stats: Stats) => any): void;
  declare function statSync(path: string): Stats;
  declare function fstat(fd: number, callback?: (err: ?ErrnoError, stats: Stats) => any): void;
  declare function fstatSync(fd: number): Stats;
  declare function lstat(path: string, callback?: (err: ?ErrnoError, stats: Stats) => any): void;
  declare function lstatSync(path: string): Stats;
  declare function link(srcpath: string, dstpath: string, callback?: (err: ?ErrnoError) => void): void;
  declare function linkSync(srcpath: string, dstpath: string): void;
  declare function symlink(srcpath: string, dtspath: string, type?: string, callback?: (err: ?ErrnoError) => void): void;
  declare function symlinkSync(srcpath: string, dstpath: string, type?: string): void;
  declare function readlink(path: string, callback: (err: ?ErrnoError, linkString: string) => void): void;
  declare function readlinkSync(path: string): string;
  declare function realpath(path: string, cache?: Object, callback?: (err: ?ErrnoError, resolvedPath: string) => void): void;
  declare function realpathSync(path: string, cache?: Object): string;
  declare function unlink(path: string, callback?: (err: ?ErrnoError) => void): void;
  declare function unlinkSync(path: string): void;

  declare type RmDirOptions = {|
    /**
     * If an `EBUSY`, `EMFILE`, `ENFILE`, `ENOTEMPTY`, or
     * `EPERM` error is encountered, Node.js will retry the operation with a linear
     * backoff wait of `retryDelay` ms longer on each try. This option represents the
     * number of retries. This option is ignored if the `recursive` option is not
     * `true`.
     * @default 0
     */
    maxRetries?: number | void,
    /**
     * @deprecated since v14.14.0 In future versions of Node.js and will trigger a warning
     * `fs.rmdir(path, { recursive: true })` will throw if `path` does not exist or is a file.
     * Use `fs.rm(path, { recursive: true, force: true })` instead.
     *
     * If `true`, perform a recursive directory removal. In
     * recursive mode soperations are retried on failure.
     * @default false
     */
    recursive?: boolean | void,
    /**
     * The amount of time in milliseconds to wait between retries.
     * This option is ignored if the `recursive` option is not `true`.
     * @default 100
     */
    retryDelay?: number | void,
  |};

  declare function rmdir(path: PathLike, callback?: (err: ?ErrnoError) => void): void;
  declare function rmdir(path: PathLike, options: RmDirOptions, callback?: (err: ?ErrnoError) => void): void;
  declare function rmdirSync(path: PathLike, options?: RmDirOptions): void;

  declare type RmOptions = {|
    /**
     * When `true`, exceptions will be ignored if `path` does not exist.
     * @default false
     */
    force?: boolean | void,
    /**
     * If an `EBUSY`, `EMFILE`, `ENFILE`, `ENOTEMPTY`, or
     * `EPERM` error is encountered, Node.js will retry the operation with a linear
     * backoff wait of `retryDelay` ms longer on each try. This option represents the
     * number of retries. This option is ignored if the `recursive` option is not
     * `true`.
     * @default 0
     */
    maxRetries?: number | void,
    /**
     * If `true`, perform a recursive directory removal. In
     * recursive mode, operations are retried on failure.
     * @default false
     */
    recursive?: boolean | void,
    /**
     * The amount of time in milliseconds to wait between retries.
     * This option is ignored if the `recursive` option is not `true`.
     * @default 100
     */
    retryDelay?: number | void,
  |};

  /**
   * Asynchronously removes files and directories (modeled on the standard POSIX `rm`utility). No arguments other than a possible exception are given to the
   * completion callback.
   * @since v14.14.0
   */
  declare function rm(path: PathLike, callback?: (err: ?ErrnoError) => void): void;
  declare function rm(path: PathLike, options: RmOptions, callback?: (err: ?ErrnoError) => void): void;

  /**
   * Synchronously removes files and directories (modeled on the standard POSIX `rm`utility). Returns `undefined`.
   * @since v14.14.0
   */
  declare function rmSync(path: PathLike, options?: RmOptions): void;

  declare function mkdir(path: string, mode?: number | {
    recursive?: boolean,
    mode?: number,
    ...
  }, callback?: (err: ?ErrnoError) => void): void;
  declare function mkdirSync(path: string, mode?: number | {
    recursive?: boolean,
    mode?: number,
    ...
  }): void;
  declare function mkdtemp(prefix: string, callback: (err: ?ErrnoError, folderPath: string) => void): void;
  declare function mkdtempSync(prefix: string): string;
  declare function readdir(
    path: string,
    options: string | { encoding?: string, withFileTypes?: false, ... },
    callback: (err: ?ErrnoError, files: Array<string>) => void
  ): void;
  declare function readdir(
    path: string,
    options: { encoding: "buffer", withFileTypes: true, ... },
    callback: (err: ?ErrnoError, files: Array<Dirent<Buffer>>) => void
  ): void;
  declare function readdir(
    path: string,
    options: { encoding?: string, withFileTypes: true, ... },
    callback: (err: ?ErrnoError, files: Array<Dirent<string>>) => void
  ): void;
  declare function readdir(
    path: string,
    callback: (err: ?ErrnoError, files: Array<string>) => void
  ): void;
  declare function readdirSync(
    path: string,
    options?: string | { encoding?: string, withFileTypes?: false, ... }
  ): Array<string>;
  declare function readdirSync(
    path: string,
    options: { encoding: "buffer", withFileTypes: true, ... }
  ): Array<Dirent<Buffer>>;
  declare function readdirSync(
    path: string,
    options?: string | { encoding?: string, withFileTypes: true, ... }
  ): Array<Dirent<string>>;
  declare function close(fd: number, callback: (err: ?ErrnoError) => void): void;
  declare function closeSync(fd: number): void;
  declare function open(
    path: string | Buffer | URL,
    flags: string | number,
    mode: number,
    callback: (err: ?ErrnoError, fd: number) => void
  ): void;
  declare function open(
    path: string | Buffer | URL,
    flags: string | number,
    callback: (err: ?ErrnoError, fd: number) => void
  ): void;
  declare function openSync(path: string | Buffer, flags: string | number, mode?: number): number;
  declare function utimes(path: string, atime: number, mtime: number, callback?: (err: ?ErrnoError) => void): void;
  declare function utimesSync(path: string, atime: number, mtime: number): void;
  declare function futimes(fd: number, atime: number, mtime: number, callback?: (err: ?ErrnoError) => void): void;
  declare function futimesSync(fd: number, atime: number, mtime: number): void;
  declare function fsync(fd: number, callback?: (err: ?ErrnoError) => void): void;
  declare function fsyncSync(fd: number): void;
  declare function write(
    fd: number,
    buffer: Buffer,
    offset: number,
    length: number,
    position: number,
    callback: (err: ?ErrnoError, write: number, buf: Buffer) => void
  ): void;
  declare function write(
    fd: number,
    buffer: Buffer,
    offset: number,
    length: number,
    callback: (err: ?ErrnoError, write: number, buf: Buffer) => void
  ): void;
  declare function write(
    fd: number,
    buffer: Buffer,
    offset: number,
    callback: (err: ?ErrnoError, write: number, buf: Buffer) => void
  ): void;
  declare function write(
    fd: number,
    buffer: Buffer,
    callback: (err: ?ErrnoError, write: number, buf: Buffer) => void
  ): void;
  declare function write(
    fd: number,
    data: string,
    position: number,
    encoding: string,
    callback: (err: ?ErrnoError, write: number, str: string) => void
  ): void;
  declare function write(
    fd: number,
    data: string,
    position: number,
    callback: (err: ?ErrnoError, write: number, str: string) => void
  ): void;
  declare function write(
    fd: number,
    data: string,
    callback: (err: ?ErrnoError, write: number, str: string) => void
  ): void;
  declare function writeSync(
    fd: number,
    buffer: Buffer,
    offset: number,
    length: number,
    position: number,
  ): number;
  declare function writeSync(
    fd: number,
    buffer: Buffer,
    offset: number,
    length: number,
  ): number;
  declare function writeSync(
    fd: number,
    buffer: Buffer,
    offset?: number,
  ): number;
  declare function writeSync(
    fd: number,
    str: string,
    position: number,
    encoding: string,
  ): number;
  declare function writeSync(
    fd: number,
    str: string,
    position?: number,
  ): number;
  declare function read(
    fd: number,
    buffer: Buffer,
    offset: number,
    length: number,
    position: ?number,
    callback: (err: ?ErrnoError, bytesRead: number, buffer: Buffer) => void
  ): void;
  declare function readSync(
    fd: number,
    buffer: Buffer,
    offset: number,
    length: number,
    position: number
  ): number;
  declare function readFile(
    path: string | Buffer | URL | number,
    callback: (err: ?ErrnoError, data: Buffer) => void
  ): void;
  declare function readFile(
    path: string | Buffer | URL | number,
    encoding: string,
    callback: (err: ?ErrnoError, data: string) => void
  ): void;
  declare function readFile(
    path: string | Buffer | URL | number,
    options: {
      encoding: string,
      flag?: string,
      ...
    },
    callback: (err: ?ErrnoError, data: string) => void
  ): void;
  declare function readFile(
    path: string | Buffer | URL | number,
    options: { encoding?: null | void, flag?: string, ... },
    callback: (err: ?ErrnoError, data: Buffer) => void
  ): void;
  declare function readFileSync(
    path: string | Buffer | URL | number
  ): Buffer;
  declare function readFileSync(
    path: string | Buffer | URL | number,
    encoding: string
  ): string;
  declare function readFileSync(path: string | Buffer | URL | number, options: {
    encoding: string,
    flag?: string,
    ...
  }): string;
  declare function readFileSync(path: string | Buffer | URL | number, options: {
    encoding?: void,
    flag?: string,
    ...
  }): Buffer;
  declare function writeFile(
    filename: string | Buffer | number,
    data: Buffer | string,
    options: string | {
      encoding?: ?string,
      mode?: number,
      flag?: string,
      ...
    },
    callback: (err: ?ErrnoError) => void
  ): void;
  declare function writeFile(
    filename: string | Buffer | number,
    data: Buffer | string,
    callback?: (err: ?ErrnoError) => void
  ): void;
  declare function writeFileSync(
    filename: string,
    data: Buffer | string,
    options?: string | {
      encoding?: ?string,
      mode?: number,
      flag?: string,
      ...
    }
  ): void;
  declare function appendFile(
    filename: string | Buffer | number,
    data: string | Buffer,
    options: string | {
      encoding?: ?string,
      mode?: number,
      flag?: string,
      ...
    },
    callback: (err: ?ErrnoError) => void
  ): void;
  declare function appendFile(
    filename: string | Buffer | number,
    data: string | Buffer,
    callback: (err: ?ErrnoError) => void
  ): void;
  declare function appendFileSync(
    filename: string | Buffer | number,
    data: string | Buffer,
    options?: string | {
      encoding?: ?string,
      mode?: number,
      flag?: string,
      ...
    }
  ): void;
  declare function watchFile(filename: string, options?: Object, listener?: (curr: Stats, prev: Stats) => void): void;
  declare function unwatchFile(filename: string, listener?: (curr: Stats, prev: Stats) => void): void;
  declare function watch(filename: string, options?: Object, listener?: (event: string, filename: string) => void): FSWatcher;
  declare function exists(path: string, callback?: (exists: boolean) => void): void;
  declare function existsSync(path: string): boolean;
  declare function access(path: string, mode?: number, callback?: (err: ?ErrnoError) => void): void;
  declare function accessSync(path: string, mode?: number): void;
  declare function createReadStream(path: string, options?: Object): ReadStream;
  declare function createWriteStream(path: string, options?: Object): WriteStream;
  declare function fdatasync(fd: number, callback: (err: ?ErrnoError) => void): void;
  declare function fdatasyncSync(fd: number): void;
  declare function copyFile(src: string, dest: string, callback: (err: ErrnoError) => void): void;
  declare function copyFile(src: string, dest: string, flags?: number, callback: (err: ErrnoError) => void): void;
  declare function copyFileSync(src: string, dest: string, flags?: number): void;

  declare type GlobOptions<WithFileTypes: boolean> = $ReadOnly<{
    /**
     * Current working directory.
     * @default process.cwd()
     */
    cwd?: string | void,
    /**
     * `true` if the glob should return paths as `Dirent`s, `false` otherwise.
     * @default false
     * @since v22.2.0
     */
    withFileTypes?: WithFileTypes,
    /**
     * Function to filter out files/directories or a
     * list of glob patterns to be excluded. If a function is provided, return
     * `true` to exclude the item, `false` to include it.
     * @default undefined
     */
    exclude?:
      | ((fileName: Node$Conditional<WithFileTypes, Dirent, string>) => boolean)
      | $ReadOnlyArray<string>,
    ...
  }>;

  /**
   * Retrieves the files matching the specified pattern.
   *
   * ```js
   * import { glob } from 'node:fs';
   *
   * glob('*.js', (err, matches) => {
   *   if (err) throw err;
   *   console.log(matches);
   * });
   * ```
   * @since v22.0.0
   */
  declare function glob(
    pattern: string | $ReadOnlyArray<string>,
    callback: (err: ?ErrnoError, matches: Array<string>) => void,
  ): void;

  declare function glob<WithFileTypes: boolean = false>(
    pattern: string | $ReadOnlyArray<string>,
    options: GlobOptions<WithFileTypes>,
    callback: (
      err: ?ErrnoError,
      matches: Node$Conditional<WithFileTypes, Array<Dirent>, Array<string>>,
    ) => void,
  ): void;

  /**
   * ```js
   * import { globSync } from 'node:fs';
   *
   * console.log(globSync('*.js'));
   * ```
   * @since v22.0.0
   * @returns paths of files that match the pattern.
   */
  declare function globSync<WithFileTypes: boolean = false>(
    pattern: string | $ReadOnlyArray<string>,
    options?: GlobOptions<WithFileTypes>,
  ): Node$Conditional<WithFileTypes, Array<Dirent>, Array<string>>;

  declare var F_OK: number;
  declare var R_OK: number;
  declare var W_OK: number;
  declare var X_OK: number;
  // new var from node 6.x
  // https://nodejs.org/dist/latest-v6.x/docs/api/fs.html#fs_fs_constants_1
  declare var constants: {
    F_OK: number, // 0
    R_OK: number, // 4
    W_OK: number, // 2
    X_OK: number, // 1
    COPYFILE_EXCL: number, // 1
    COPYFILE_FICLONE: number, // 2
    COPYFILE_FICLONE_FORCE: number, // 4
    O_RDONLY: number, // 0
    O_WRONLY: number, // 1
    O_RDWR: number, // 2
    S_IFMT: number, // 61440
    S_IFREG: number, // 32768
    S_IFDIR: number, // 16384
    S_IFCHR: number, // 8192
    S_IFBLK: number, // 24576
    S_IFIFO: number, // 4096
    S_IFLNK: number, // 40960
    S_IFSOCK: number, // 49152
    O_CREAT: number, // 64
    O_EXCL: number, // 128
    O_NOCTTY: number, // 256
    O_TRUNC: number, // 512
    O_APPEND: number, // 1024
    O_DIRECTORY: number, // 65536
    O_NOATIME: number, // 262144
    O_NOFOLLOW: number, // 131072
    O_SYNC: number, // 1052672
    O_DSYNC: number, // 4096
    O_SYMLINK: number, // 2097152
    O_DIRECT: number, // 16384
    O_NONBLOCK: number, // 2048
    S_IRWXU: number, // 448
    S_IRUSR: number, // 256
    S_IWUSR: number, // 128
    S_IXUSR: number, // 64
    S_IRWXG: number, // 56
    S_IRGRP: number, // 32
    S_IWGRP: number, // 16
    S_IXGRP: number, // 8
    S_IRWXO: number, // 7
    S_IROTH: number, // 4
    S_IWOTH: number, // 2
    S_IXOTH: number, // 1
    ...
  };

  declare type BufferEncoding =
    | 'buffer'
    | { encoding: 'buffer', ... };
  declare type EncodingOptions = { encoding?: string, ... };
  declare type EncodingFlag = EncodingOptions & { flag?: string, ... };
  declare type WriteOptions = EncodingFlag & { mode?: number, ... };
  declare type RemoveOptions = { force?: boolean, maxRetries?: number, recursive?: boolean, retryDelay?: number, ... };
  declare class FileHandle {
    appendFile(data: string | Buffer, options: WriteOptions | string): Promise<void>;
    chmod(mode: number): Promise<void>;
    chown(uid: number, guid: number): Promise<void>;
    close(): Promise<void>;
    datasync(): Promise<void>;
    fd: number;
    read<T: Buffer | Uint8Array>(
      buffer: T,
      offset: number,
      length: number,
      position: number
    ): Promise<{
      bytesRead: number,
      buffer: T,
      ...
    }>;
    readFile(options: EncodingFlag): Promise<Buffer>;
    readFile(options: string): Promise<string>;
    stat(): Promise<Stats>;
    sync(): Promise<void>;
    truncate(len?: number): Promise<void>;
    utimes(atime: number | string | Date, mtime: number | string | Date): Promise<void>;
    write(buffer: Buffer | Uint8Array, offset: number, length: number, position: number): Promise<void>;
    writeFile(data: string | Buffer | Uint8Array, options: WriteOptions | string): Promise<void>;
  }

  declare type FSPromisePath = string | Buffer | URL;
  declare type FSPromise = {
    access(path: FSPromisePath, mode?: number): Promise<void>,
    appendFile(path: FSPromisePath | FileHandle, data: string | Buffer, options?: WriteOptions | string): Promise<void>,
    chmod(path: FSPromisePath, mode: number): Promise<void>,
    chown(path: FSPromisePath, uid: number, gid: number): Promise<void>,
    copyFile(src: FSPromisePath, dest: FSPromisePath, flags?: number): Promise<void>,
    fchmod(filehandle: FileHandle, mode: number): Promise<void>,
    fchown(filehandle: FileHandle, uid: number, guid: number): Promise<void>,
    fdatasync(filehandle: FileHandle): Promise<void>,
    fstat(filehandle: FileHandle): Promise<Stats>,
    fsync(filehandle: FileHandle): Promise<void>,
    ftruncate(filehandle: FileHandle, len?: number): Promise<void>,
    futimes(filehandle: FileHandle, atime: number | string | Date, mtime: number | string | Date): Promise<void>,
    lchmod(path: FSPromisePath, mode: number): Promise<void>,
    glob<WithFileTypes: boolean = false>(
      pattern: string | $ReadOnlyArray<string>,
      options?: GlobOptions<WithFileTypes>,
    ): Node$Conditional<
      WithFileTypes,
      AsyncIterator<Dirent>,
      AsyncIterator<string>,
    >,
    lchown(path: FSPromisePath, uid: number, guid: number): Promise<void>,
    link(existingPath: FSPromisePath, newPath: FSPromisePath): Promise<void>,
    lstat(path: FSPromisePath): Promise<Stats>,
    mkdir(path: FSPromisePath, mode?: number | {
      recursive?: boolean,
      mode?: number,
      ...
    }): Promise<void>,
    mkdtemp(prefix: string, options?: EncodingOptions): Promise<string>,
    open(path: FSPromisePath, flags?: string | number, mode?: number): Promise<FileHandle>,
    read<T: Buffer | Uint8Array>(
      filehandle: FileHandle,
      buffer: T,
      offset: number,
      length: number,
      position?: number
    ): Promise<{
      bytesRead: number,
      buffer: T,
      ...
    }>,
    readdir:
      & ((path: FSPromisePath, options: string | { encoding?: string, withFileTypes?: false, ... }) => Promise<Array<string>>)
      & ((path: FSPromisePath, options: { encoding: "buffer", withFileTypes: true, ... }) => Promise<Array<Dirent<Buffer>>>)
      & ((path: FSPromisePath, options: { encoding?: string, withFileTypes: true, ... }) => Promise<Array<Dirent<string>>>)
      & ((path: FSPromisePath) => Promise<Array<string>>),
    readFile:
      & ((path: FSPromisePath | FileHandle, options: string) => Promise<string>)
      & ((path: FSPromisePath | FileHandle, options?: EncodingFlag) => Promise<Buffer>),
    readlink:
      & ((path: FSPromisePath, options: BufferEncoding) => Promise<Buffer>)
      & ((path: FSPromisePath, options?: string | EncodingOptions) => Promise<string>),
    realpath:
      & ((path: FSPromisePath, options: BufferEncoding) => Promise<Buffer>)
      & ((path: FSPromisePath, options?: string | EncodingOptions) => Promise<string>),
    rename(oldPath: FSPromisePath, newPath: FSPromisePath): Promise<void>,
    rm(path: FSPromisePath, options?: RemoveOptions): Promise<void>,
    rmdir(path: FSPromisePath): Promise<void>,
    stat(path: FSPromisePath): Promise<Stats>,
    symlink(target: FSPromisePath, path: FSPromisePath, type?: 'dir' | 'file' | 'junction'): Promise<void>,
    truncate(path: FSPromisePath, len?: number): Promise<void>,
    unlink(path: FSPromisePath): Promise<void>,
    utimes(path: FSPromisePath, atime: number | string | Date, mtime: number | string | Date): Promise<void>,
    write<T: Buffer | Uint8Array>(
      filehandle: FileHandle,
      buffer: T,
      offset: number,
      length: number,
      position?: number
    ): Promise<{
      bytesRead: number,
      buffer: T,
      ...
    }>,
    writeFile(
      FSPromisePath | FileHandle,
      data: string | Buffer | Uint8Array,
      options?: string | WriteOptions
    ): Promise<void>,
    ...
  }

  declare var promises: FSPromise;
}
