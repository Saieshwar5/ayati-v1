import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { expectDefined } from "@openclaw/normalization-core/expect";
import { describe, expect, it, vi } from "vitest";
import {
  buildIosCatalog,
  checkAppleAppI18n,
  findAmbiguousRuntimeInterpolations,
  infoPlistTranslationCandidates,
  selectInfoPlistTranslation,
  serializeAppleCatalog,
  verifyAppleAppI18n,
} from "../../scripts/apple-app-i18n.ts";
import {
  type NativeI18nInventoryEntry,
  parseNativeI18nInventory,
} from "../../scripts/native-i18n-inventory.ts";
import { NATIVE_I18N_LOCALES } from "../../scripts/native-i18n-locales.ts";

const probe = vi.hoisted(() => ({
  source: "",
  missingPath: "",
  readPaths: [] as string[],
  catalogs: new Map<string, string>(),
  paths: [
    "apps/ios/Sources/Gateway/ExecApprovalPromptDialog.swift",
    "apps/shared/OpenClawKit/Sources/OpenClawChatUI/ChatComposer+Controls.swift",
    "apps/shared/OpenClawKit/Sources/OpenClawKit/GatewayDiscoveryStatusText.swift",
  ],
}));

vi.mock("node:fs/promises", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs/promises")>();
  return {
    ...actual,
    // Synthetic calls are opt-in and limited to production-source reads; all other I/O stays real.
    readFile: async (...args: Parameters<typeof actual.readFile>) => {
      const file = typeof args[0] === "string" ? args[0].replaceAll("\\", "/") : "";
      probe.readPaths.push(file);
      if (probe.missingPath && file.endsWith("/" + probe.missingPath)) {
        throw Object.assign(new Error(`Missing required mobile input: ${probe.missingPath}`), {
          code: "ENOENT",
        });
      }
      const catalog = probe.catalogs.get(path.resolve(file));
      if (catalog !== undefined) {
        return catalog;
      }
      const source = await actual.readFile(...args);
      return probe.source &&
        typeof source === "string" &&
        probe.paths.some((entry) => file.endsWith("/" + entry))
        ? source + "\n" + probe.source
        : source;
    },
  };
});

describe("Apple app i18n catalogs", () => {
  it("verification rejects raw interpolation in retained mobile and shared sources", async () => {
    const gates = [() => verifyAppleAppI18n()];
    probe.readPaths.length = 0;
    try {
      probe.source = 'Label("Expires in \\(minutes) minutes", systemImage: "clock")';
      const diagnostic = [
        "Apple i18n runtime interpolation bypasses generated catalog coverage:",
        ...probe.paths
          .toSorted()
          .map((entry) => path.normalize(entry) + ": interpolated SwiftUI text literal"),
      ].join("\n");
      for (const gate of gates) {
        await expect(gate()).rejects.toThrow(new Error(diagnostic));
      }
      probe.source = [
        "let minutes: Int = 3",
        'Label(String(format: String(localized: "Expires in %lld minutes"), minutes), systemImage: "clock")',
        'Text(verbatim: "\\(name) — \\(minutes)")',
        "let count: Int = 2",
        'String(AttributedString(localized: "^[\\(count) message](inflect: true)").characters)',
      ].join("\n");
      for (const gate of gates) {
        await expect(gate()).resolves.toBeUndefined();
      }
      expect(probe.readPaths.some((file) => file.includes("/apps/macos/"))).toBe(false);
    } finally {
      probe.source = "";
      probe.readPaths.length = 0;
    }
  });

  it.each([
    "apps/ios/Sources/Gateway/ExecApprovalPromptDialog.swift",
    "apps/ios/Resources/Localizable.xcstrings",
  ])("rejects a missing required mobile input: %s", async (missingPath) => {
    probe.missingPath = missingPath;
    try {
      await expect(verifyAppleAppI18n()).rejects.toMatchObject({ code: "ENOENT" });
      await expect(checkAppleAppI18n()).rejects.toMatchObject({ code: "ENOENT" });
    } finally {
      probe.missingPath = "";
    }
  });

  it("ships translated runtime keys for iOS, watchOS, and explicit localized calls", async () => {
    const catalog = JSON.parse(
      await readFile("apps/ios/Resources/Localizable.xcstrings", "utf8"),
    ) as {
      strings: Record<
        string,
        { localizations?: Record<string, { stringUnit?: { state?: string; value?: string } }> }
      >;
    };

    for (const key of [
      "^[%lld approval](inflect: true) waiting",
      "Approval needed",
      "Agent: %@",
      "Connect a nearby Gateway",
      "Talk to Claw",
      "Expires in %@",
      "Pending review",
      "Secure connection is required for this host.",
      "TLS required",
      "Use only on a trusted private network.",
    ]) {
      const entry = catalog.strings[key];
      expect(entry, key).toBeDefined();
      const localizedValues: string[] = [];
      for (const locale of ["en", ...NATIVE_I18N_LOCALES]) {
        const unit = entry?.localizations?.[locale]?.stringUnit;
        expect(unit?.value, `${key}:${locale}`).toBeTruthy();
        if (locale !== "en" && unit?.value) {
          localizedValues.push(unit.value);
        }
      }
      expect(
        localizedValues.some((value) => value !== key),
        key,
      ).toBe(true);
    }
  });

  it("derives shared discovery status coverage into the iOS catalog", async () => {
    const inventory = parseNativeI18nInventory(
      await readFile("apps/.i18n/native-source.json", "utf8"),
    );
    const build = buildIosCatalog(
      { sourceLanguage: "en", strings: {}, version: "1.0" },
      inventory,
      [],
    );

    expect(Object.keys(build.catalog.strings ?? {})).toEqual(
      expect.arrayContaining(["Searching…", "Stopped", "Waiting"]),
    );
  });

  it("warns only when obsolete Apple keys are the entire catalog drift", async () => {
    const inventory = parseNativeI18nInventory(
      await readFile("apps/.i18n/native-source.json", "utf8"),
    );
    const translations = await Promise.all(
      NATIVE_I18N_LOCALES.map(async (locale) =>
        JSON.parse(await readFile(`apps/.i18n/native/${locale}.json`, "utf8")),
      ),
    );
    const catalogs = await Promise.all(
      ([["apps/ios/Resources/Localizable.xcstrings", buildIosCatalog]] as const).map(
        async ([file, buildCatalog]) => {
          const filePath = path.resolve(file);
          const build = buildCatalog(
            JSON.parse(await readFile(filePath, "utf8")),
            inventory,
            translations,
          );
          return { filePath, catalog: build.catalog };
        },
      ),
    );
    try {
      // Source PRs can await generation; keep this fixture's active resources current.
      for (const { filePath, catalog } of catalogs) {
        probe.catalogs.set(filePath, serializeAppleCatalog(catalog));
      }
      for (const { filePath, catalog } of catalogs) {
        const warnings: string[] = [];
        const options = { reportObsolete: (message: string) => warnings.push(message) };
        const strings = expectDefined(catalog.strings, "active catalog strings");
        const activeKey = expectDefined(
          Object.keys(strings).find((key) => key.includes("%@")),
          "active format key",
        );
        const obsolete: typeof catalog & { strings: typeof strings } = {
          ...catalog,
          strings: {
            ...strings,
            "Retired synthetic %@": {
              localizations: { en: { stringUnit: { state: "new", value: "" } } },
            },
            "Retired empty title": {},
          },
        };
        const serialized = serializeAppleCatalog(obsolete);
        probe.catalogs.set(filePath, serialized);
        await expect(checkAppleAppI18n()).rejects.toThrow("is stale");
        warnings.length = 0;
        await expect(checkAppleAppI18n(options)).resolves.toBeUndefined();
        expect(warnings).toEqual([
          `Apple obsolete catalog rows: ${path.relative(process.cwd(), filePath).replaceAll("\\", "/")} (keys=2)`,
        ]);

        for (const ineligibleRow of [
          "null",
          "42",
          '{"localizations":null}',
          '{"localizations":{"en":42}}',
          '{"localizations":{"en":{"stringUnit":null}}}',
          '{"localizations":{"en":{"stringUnit":{"state":"new"}}}}',
          '{"localizations":{"en":{"stringUnit":{"state":42,"value":"Retired"}}}}',
          '{"comment":42}',
          '{"localizations":{"en":{"variations":{}}}}',
        ]) {
          probe.catalogs.set(
            filePath,
            serialized.replace(
              '"Retired empty title": {}',
              `"Retired empty title": ${ineligibleRow}`,
            ),
          );
          await expect(checkAppleAppI18n(options)).rejects.toThrow();
        }

        const missingKey = structuredClone(obsolete);
        delete missingKey.strings[activeKey];
        const missingLocale = structuredClone(obsolete);
        delete missingLocale.strings[activeKey]?.localizations?.de;
        const placeholderDrift = structuredClone(obsolete);
        expectDefined(
          placeholderDrift.strings[activeKey]?.localizations?.de?.stringUnit,
          "German format unit",
        ).value = "Missing format argument";
        const metadataDrift = structuredClone(obsolete);
        expectDefined(metadataDrift.strings[activeKey], "active catalog entry").comment =
          "Unexpected metadata";
        for (const invalid of [
          serializeAppleCatalog(missingKey),
          serializeAppleCatalog(missingLocale),
          serializeAppleCatalog(placeholderDrift),
          serializeAppleCatalog(metadataDrift),
          serializeAppleCatalog({ ...obsolete, sourceLanguage: "fr" }),
          serializeAppleCatalog({ ...obsolete, version: "2.0" }),
          `${serialized}\n`,
          `${serialized}malformed\n`,
        ]) {
          probe.catalogs.set(filePath, invalid);
          await expect(checkAppleAppI18n(options)).rejects.toThrow();
        }
        probe.catalogs.set(filePath, serializeAppleCatalog(catalog));
      }
    } finally {
      probe.catalogs.clear();
    }
  });

  it("serializes one complete localization key per line without losing nested metadata", () => {
    const catalog = {
      sourceLanguage: "en",
      strings: {
        Plain: {
          localizations: {
            en: { stringUnit: { state: "translated", value: "Plain" } },
          },
        },
        "Rich %@": {
          comment: "Translator context",
          extractionState: "manual",
          shouldTranslate: false,
          localizations: {
            en: {
              substitutions: {
                count: {
                  variations: {
                    plural: {
                      one: { stringUnit: { state: "translated", value: "One %@" } },
                      other: { stringUnit: { state: "new", value: "%@ items" } },
                    },
                  },
                },
              },
              stringUnit: { state: "translated", value: "Rich %@" },
            },
          },
        },
      },
      version: "1.0",
    };

    const serialized = serializeAppleCatalog(catalog);
    const lines = serialized.trimEnd().split("\n");

    expect(JSON.parse(serialized)).toEqual(catalog);
    expect(lines).toHaveLength(Object.keys(catalog.strings).length + 6);
    expect(lines[3]).toBe(`    "Plain": ${JSON.stringify(catalog.strings.Plain)},`);
    expect(lines[4]).toBe(`    "Rich %@": ${JSON.stringify(catalog.strings["Rich %@"])}`);
    expect(serialized.endsWith("\n")).toBe(true);
  });

  it("routes merged sites by coupled path and kind while preserving shipped translations", () => {
    const coveredIosEntries: NativeI18nInventoryEntry[] = [
      { kind: "ui-call-concatenated", source: "Call concatenated" },
      {
        kind: "ui-localized-call-concatenated",
        source:
          "Older generated approvals are inactive because they were not tied to a working directory. Manual rules are unchanged.",
      },
      { kind: "ui-modifier-concatenated", source: "Modifier concatenated" },
      { kind: "ui-modifier-multiline", source: "Modifier multiline" },
      { kind: "ui-named-argument-concatenated", source: "Named argument concatenated" },
    ].map(({ kind, source }, index) => ({
      id: `native.apple.concatenated.${index}`,
      source,
      surface: "apple",
      sites: [{ kind, path: "apps/ios/Sources/Example.swift" }],
    }));
    const inventory: NativeI18nInventoryEntry[] = [
      {
        id: "native.apple.connect",
        source: "Connect now",
        surface: "apple",
        sites: [
          { kind: "ui-call", path: "apps/ios/Sources/Example.swift" },
          { kind: "ui-call", path: "apps/macos/Sources/OpenClaw/Example.swift" },
        ],
      },
      {
        id: "native.apple.decoy",
        source: "Do not catalog",
        surface: "apple",
        sites: [
          { kind: "plist-string", path: "apps/ios/Sources/Info.plist" },
          { kind: "ui-call", path: "outside/Example.swift" },
        ],
      },
      {
        id: "native.apple.retired-desktop",
        source: "Retired desktop title",
        surface: "apple",
        sites: [{ kind: "ui-call", path: "apps/macos/Sources/OpenClaw/Example.swift" }],
      },
      ...coveredIosEntries,
    ];
    const existing = {
      sourceLanguage: "en",
      strings: {
        "Connect now": {
          localizations: {
            de: { stringUnit: { state: "translated", value: "Jetzt verbinden" } },
          },
        },
      },
    };
    const translations = [
      {
        version: 2,
        locale: "fr",
        translations: { "native.apple.connect": "Se connecter" },
      },
    ];
    const ios = buildIosCatalog(existing, inventory, translations);

    expect(ios.catalog.strings?.["Connect now"]?.localizations?.de?.stringUnit?.value).toBe(
      "Jetzt verbinden",
    );
    expect(ios.catalog.strings?.["Connect now"]?.localizations?.fr?.stringUnit).toEqual({
      state: "translated",
      value: "Se connecter",
    });
    expect(ios.catalog.strings?.["Connect now"]?.localizations?.es?.stringUnit).toEqual({
      state: "new",
      value: "Connect now",
    });
    expect(ios.catalog.strings?.["Do not catalog"]).toBeUndefined();
    expect(Object.keys(ios.catalog.strings ?? {})).toEqual(
      expect.arrayContaining(coveredIosEntries.map((entry) => entry.source)),
    );
    expect(ios.catalog.strings?.["Retired desktop title"]).toBeUndefined();
    expect(ios.contradictions).toEqual([]);
  });

  it.each([
    ["iOS", buildIosCatalog, "apps/ios/Sources/Example.swift"],
    ["shared iOS", buildIosCatalog, "apps/shared/OpenClawKit/Sources/OpenClawChatUI/Example.swift"],
  ] as const)(
    "converts only constrained inflected counts into typed %s catalog keys",
    (_platform, buildCatalog, sourcePath) => {
      const source = "^[\\(count) entry](inflect: true)";
      const translated = "^[\\(count) Eintrag](inflect: true)";
      const build = buildCatalog(
        { sourceLanguage: "en", strings: {} },
        [
          {
            id: "native.apple.count",
            source,
            sites: [{ kind: "ui-localized-call", path: sourcePath }],
            surface: "apple",
          },
          {
            id: "native.apple.mixed-count",
            source: "\\(name) has " + source,
            sites: [{ kind: "ui-localized-call", path: sourcePath }],
            surface: "apple",
          },
        ],
        [
          {
            version: 2,
            locale: "de",
            translations: { "native.apple.count": translated },
          },
        ],
      );

      const key = "^[%lld entry](inflect: true)";
      expect(Object.keys(build.catalog.strings ?? {})).toEqual([key]);
      expect(build.catalog.strings?.[key]?.localizations?.en?.stringUnit?.value).toBe(key);
      expect(build.catalog.strings?.[key]?.localizations?.de?.stringUnit?.value).toBe(
        "^[%lld Eintrag](inflect: true)",
      );
    },
  );

  it("keeps dynamic Watch content verbatim and requires explicit localization", async () => {
    const watch = await readFile("apps/ios/WatchApp/Sources/WatchInboxView.swift", "utf8");

    expect(watch).not.toContain("WatchTextValue: ExpressibleByStringLiteral");
    expect(watch).toContain("accessory: .verbatim(self.store.talkSummaryText)");
  });

  it("rejects interpolated runtime copy across every supported Swift syntax", () => {
    const source = String.raw`
      let key = LocalizedStringKey("Hello \(name)")
      let detail = String(localized: """
        Welcome \(name)
        """)
      Toggle("Enable \(feature)", isOn: $enabled)
      Menu("""
        Open \(item)
        """) {}
      view.accessibilityHint("""
        Select \(item)
        """)
    `;

    expect(findAmbiguousRuntimeInterpolations(source)).toEqual([
      "interpolated localized resource",
      "interpolated multiline localized resource",
      "interpolated SwiftUI text literal",
      "interpolated multiline SwiftUI text literal",
      "interpolated multiline SwiftUI modifier literal",
    ]);
  });

  it("generates only localized usage descriptions for every shipped iOS target", async () => {
    const french = await readFile("apps/ios/Sources/fr.lproj/InfoPlist.strings", "utf8");
    const watchChinese = await readFile(
      "apps/ios/WatchApp/zh-Hans.lproj/InfoPlist.strings",
      "utf8",
    );
    const shareGerman = await readFile(
      "apps/ios/ShareExtension/de.lproj/InfoPlist.strings",
      "utf8",
    );
    const activityJapanese = await readFile(
      "apps/ios/ActivityWidget/ja.lproj/InfoPlist.strings",
      "utf8",
    );

    expect(french).toContain('"NSCameraUsageDescription" = ');
    expect(french).toContain('"NSMicrophoneUsageDescription" = ');
    expect(french).toContain('"NSHealthUpdateUsageDescription" = ');
    expect(watchChinese).toContain('"NSLocalNetworkUsageDescription" = ');
    expect(shareGerman.trim()).toBe("");
    expect(activityJapanese.trim()).toBe("");

    for (const root of [
      "apps/ios/Sources",
      "apps/ios/WatchApp",
      "apps/ios/ShareExtension",
      "apps/ios/ActivityWidget",
    ]) {
      const localeDirs = (await readdir(root, { withFileTypes: true })).filter(
        (entry) => entry.isDirectory() && entry.name.endsWith(".lproj"),
      );
      expect(localeDirs).toHaveLength(NATIVE_I18N_LOCALES.length);
      for (const localeDir of localeDirs) {
        const localizedPlist = await readFile(
          path.join(root, localeDir.name, "InfoPlist.strings"),
          "utf8",
        );
        expect(localizedPlist).not.toContain("CFBundleDisplayName");
        expect(localizedPlist).not.toMatch(/\$\([^)]*\)|\$\{[^}]*\}/u);
      }
    }
  });

  it("refreshes InfoPlist copy from translations for the current source", () => {
    expect(
      selectInfoPlistTranslation(
        "Use the camera to scan setup codes.",
        ["Utilisez l’appareil photo pour scanner les codes de configuration."],
        {
          source: "Old camera purpose.",
          value: "Ancienne description de la caméra.",
        },
      ),
    ).toBe("Utilisez l’appareil photo pour scanner les codes de configuration.");
    expect(
      selectInfoPlistTranslation("OpenClaw Share", [], {
        source: "OpenClaw Share",
        value: "OpenClaw Partager",
      }),
    ).toBe("OpenClaw Partager");
    expect(
      selectInfoPlistTranslation(
        "Use the camera to scan setup codes.",
        ["Use the camera to scan setup codes."],
        {
          source: "Use the camera to scan setup codes.",
          value: "Utilisez l’appareil photo pour scanner les codes de configuration.",
        },
      ),
    ).toBe("Utilisez l’appareil photo pour scanner les codes de configuration.");
    expect(
      selectInfoPlistTranslation("Use the camera for video calls.", [], {
        source: "Use the camera to scan setup codes.",
        value: "Utilisez l’appareil photo pour scanner les codes de configuration.",
      }),
    ).toBe("Use the camera for video calls.");
  });

  it("selects InfoPlist candidates by stable ID instead of shared source text", () => {
    const source = "Use the camera to scan setup codes.";
    const artifact = {
      version: 2,
      locale: "fr",
      translations: {
        "native.apple.camera": "Utilisez l’appareil photo pour scanner les codes de configuration.",
        "native.apple.unrelated": "Traduction pour un autre contexte.",
      },
    };

    expect(infoPlistTranslationCandidates(artifact, "native.apple.camera", source)).toEqual([
      "Utilisez l’appareil photo pour scanner les codes de configuration.",
    ]);
  });
});
