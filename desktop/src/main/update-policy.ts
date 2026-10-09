/**
 * The update policy (architecture §14), as pure functions so it is tested without Electron: which
 * builds update and how, whether the person allows the check, and where a release's page is.
 * `updater.ts` applies it.
 */

/**
 * Where releases are published. PLACEHOLDER until the repository's final home is chosen with the
 * trademark check (U-3); keep it in step with `publish` in `electron-builder.yml`.
 */
export const RELEASES_URL = "https://github.com/unfold-placeholder/unfold/releases";

/** What the updater does in this build. */
export type UpdateMode =
  | { readonly kind: "off"; readonly reason: "development" | "not_a_release_build" | "unsigned" | "unsupported_package" }
  /** Download, verify, and install (macOS, Windows, the AppImage). */
  | { readonly kind: "install" }
  /** Only tell the person a new version exists (`.deb`, `.rpm`: the system's package manager installs). */
  | { readonly kind: "notify"; readonly packageType: "deb" | "rpm" };

/** The facts the update mode depends on, gathered by `currentUpdateEnvironment` (updater.ts). */
export interface UpdateEnvironment {
  readonly isPackaged: boolean;
  readonly platform: NodeJS.Platform;
  /** The build came from the release workflow (`unfoldReleaseBuild: true` in its `package.json`). */
  readonly releaseBuild: boolean;
  /** `$APPIMAGE`, set by the AppImage runtime. */
  readonly appImage: string | undefined;
  /** `<resources>/package-type`, which electron-builder writes into `.deb` and `.rpm` builds. */
  readonly packageType: string | null;
  /** `app-update.yml` names the Authenticode publisher updates must be signed by (Windows). */
  readonly publisherPinned: boolean;
}

export function updateMode(env: UpdateEnvironment): UpdateMode {
  if (!env.isPackaged) {
    return { kind: "off", reason: "development" };
  }
  if (!env.releaseBuild) {
    return { kind: "off", reason: "not_a_release_build" };
  }
  switch (env.platform) {
    case "darwin":
      return { kind: "install" };
    case "win32":
      // Without a publisher name electron-updater would skip the signature check.
      return env.publisherPinned ? { kind: "install" } : { kind: "off", reason: "unsigned" };
    case "linux":
      if (env.appImage !== undefined && env.appImage !== "") {
        return { kind: "install" };
      }
      if (env.packageType === "deb" || env.packageType === "rpm") {
        return { kind: "notify", packageType: env.packageType };
      }
      return { kind: "off", reason: "unsupported_package" };
    default:
      return { kind: "off", reason: "unsupported_package" };
  }
}

/** The `updates.enabled` preference from a `get_preferences` result, or null if it isn't there. */
export function updatesEnabled(preferences: unknown): boolean | null {
  if (typeof preferences !== "object" || preferences === null) {
    return null;
  }
  const updates = (preferences as Record<string, unknown>)["updates"];
  if (typeof updates !== "object" || updates === null) {
    return null;
  }
  const enabled = (updates as Record<string, unknown>)["enabled"];
  return typeof enabled === "boolean" ? enabled : null;
}

const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/** The release page for `version`, or the latest release if the version looks malformed. */
export function releasePageUrl(version: string): string {
  return VERSION.test(version) ? `${RELEASES_URL}/tag/v${version}` : `${RELEASES_URL}/latest`;
}

/** Whether `app-update.yml`'s text pins a Windows publisher name. */
export function pinsPublisher(appUpdateYml: string): boolean {
  return /^publisherName:\s*\S/m.test(appUpdateYml);
}
