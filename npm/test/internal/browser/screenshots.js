// @flow
import { readFile, writeFile, mkdir, realpath, lstat } from "node:fs/promises";
import path from "node:path";
import { PNG } from "pngjs";
import pixelmatch from "pixelmatch";

export type ScreenshotOptions = {|
  readonly root?: string,
  readonly baselines?: string,
  readonly threshold?: number,
|};

/** A screenshot that matched its baseline, or was recorded as the new one. */
export type ScreenshotResult = {|
  readonly baseline: string,
  readonly updated: boolean,
  readonly differentPixels: number,
|};

/** Baselines stay under the project's configured directory, including through symlinks. */
export async function compareScreenshot(
  data: string,
  name: string,
  options: ScreenshotOptions = {},
): Promise<ScreenshotResult> {
  if (
    typeof name !== "string" ||
    !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,119}$/.test(name) ||
    name.includes("..") ||
    /\.(actual|diff)$/.test(name)
  )
    throw new Error("screenshot name must be a simple file name without traversal");
  const vrt = JSON.parse(process.env.UF_VRT_CONFIG ?? "{}");
  const root = await realpath(options.root ?? process.env.UF_PROJECT_ROOT ?? process.cwd());
  const directory = path.resolve(root, options.baselines ?? vrt.baselines ?? "__uf_vrt__");
  if (!directory.startsWith(root + path.sep))
    throw new Error("vrt.baselines must stay inside the test project");
  // Check each existing ancestor before creating anything through it.
  let ancestor = root;
  for (const segment of path.relative(root, directory).split(path.sep)) {
    ancestor = path.join(ancestor, segment);
    try {
      if ((await lstat(ancestor)).isSymbolicLink())
        throw new Error("vrt.baselines cannot traverse a symlink");
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  await mkdir(directory, { recursive: true });
  const baseline = path.join(directory, `${name}.png`);
  try {
    if ((await lstat(baseline)).isSymbolicLink())
      throw new Error("screenshot baseline cannot be a symlink");
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const bytes = Buffer.from(data, "base64");
  const actual = PNG.sync.read(bytes);
  const threshold = options.threshold ?? vrt.threshold ?? 0;
  if (!Number.isInteger(threshold) || threshold < 0)
    throw new Error("vrt.threshold is the maximum number of different pixels");
  if (process.env.UF_UPDATE_SNAPSHOTS === "1") {
    await writeFile(baseline, bytes);
    return { baseline, updated: true, differentPixels: 0 };
  }
  let expected;
  try {
    expected = PNG.sync.read(await readFile(baseline));
  } catch (error) {
    if (error.code === "ENOENT")
      throw new Error(`missing screenshot baseline ${baseline}; record it with uf test -u`);
    throw error;
  }
  const width = Math.max(actual.width, expected.width),
    height = Math.max(actual.height, expected.height);
  const diff = new PNG({ width, height });
  const sameSize = actual.width === expected.width && actual.height === expected.height;
  let differentPixels;
  if (sameSize)
    differentPixels = pixelmatch(actual.data, expected.data, diff.data, width, height, {
      threshold: 0.1,
      includeAA: true,
    });
  else {
    differentPixels = width * height;
    for (let i = 0; i < diff.data.length; i += 4) {
      diff.data[i] = 255;
      diff.data[i + 3] = 255;
    }
  }
  if (!sameSize || differentPixels > threshold) {
    const actualFile = path.join(directory, `${name}.actual.png`);
    const diffFile = path.join(directory, `${name}.diff.png`);
    for (const file of [actualFile, diffFile]) {
      try {
        if ((await lstat(file)).isSymbolicLink())
          throw new Error("screenshot output cannot be a symlink");
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
      }
    }
    await writeFile(actualFile, bytes);
    await writeFile(diffFile, PNG.sync.write(diff));
    throw new Error(
      `screenshot ${name}: ${differentPixels} pixels differ (allowed ${threshold})${sameSize ? "" : "; dimensions changed"}; diff: ${diffFile}`,
    );
  }
  return { baseline, updated: false, differentPixels };
}
