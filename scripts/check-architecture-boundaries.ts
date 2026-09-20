import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, normalize, relative, sep } from "node:path";
import ts from "typescript";

const SCAN_ROOTS = ["src/app", "src/features", "src/shared", "src/platform"] as const;
const DEFAULT_CAPABILITY_PATH = "src-tauri/capabilities/default.json";
const WIDGET_CAPABILITY_PATH = "src-tauri/capabilities/widget.json";

interface SourceFile {
  path: string;
  content: string;
}

interface ArchitectureViolation {
  path: string;
  line: number;
  rule: string;
  text: string;
}

function normalizePath(path: string) {
  return path.split(sep).join("/");
}

function normalizeImportPath(fromFile: string, specifier: string) {
  if (specifier.startsWith(".")) {
    return normalizePath(normalize(join(dirname(fromFile), specifier)));
  }

  if (specifier.startsWith("@/")) {
    return `src/${specifier.slice(2)}`;
  }

  return specifier;
}

function importSpecifier(node: ts.Node): string | undefined {
  let specifier: ts.Node | undefined;
  if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
    specifier = node.moduleSpecifier;
  } else if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
    specifier = node.arguments[0];
  } else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) {
    specifier = node.argument.literal;
  } else if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)) {
    specifier = node.moduleReference.expression;
  }

  // Computed imports have no statically known owner; only literal module paths
  // are checked here. Comments and ordinary strings are not dependencies.
  return specifier && ts.isStringLiteralLike(specifier) ? specifier.text : undefined;
}

function collectSourceFiles(root: string): SourceFile[] {
  const files: SourceFile[] = [];

  function walk(path: string) {
    const stats = statSync(path);
    if (stats.isDirectory()) {
      for (const entry of readdirSync(path)) {
        walk(join(path, entry));
      }
      return;
    }

    if (!/\.(ts|tsx)$/.test(path)) {
      return;
    }

    files.push({
      path: normalizePath(relative(process.cwd(), path)),
      content: readFileSync(path, "utf8"),
    });
  }

  walk(root);
  return files;
}

function isFeatureComponentOrHook(path: string) {
  return /^src\/features\/[^/]+\/(components|hooks)\//.test(path);
}

function isSharedSource(path: string) {
  return /^src\/shared\//.test(path);
}

function isPlatformSource(path: string) {
  return /^src\/platform\//.test(path);
}

function isAppComponentOrHook(path: string) {
  return /^src\/app\/(components|hooks)\//.test(path);
}

function isAppComponent(path: string) {
  return /^src\/app\/components\//.test(path);
}

function findArchitectureViolations(files: SourceFile[]): ArchitectureViolation[] {
  const violations: ArchitectureViolation[] = [];

  for (const file of files) {
    const source = ts.createSourceFile(file.path, file.content, ts.ScriptTarget.Latest, true);
    function visit(node: ts.Node) {
      function report(rule: string) {
        violations.push({
          path: file.path,
          line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1,
          rule,
          text: node.getText(source).replace(/\s+/g, " "),
        });
      }

      const specifier = importSpecifier(node);
      if (specifier !== undefined) {
        const importedPath = normalizeImportPath(file.path, specifier);
        if (isSharedSource(file.path) && /^src\/app\//.test(importedPath)) {
          report("shared-no-app-import");
        }

        if (isSharedSource(file.path) && /^src\/features\//.test(importedPath)) {
          report("shared-no-feature-import");
        }

        if (isSharedSource(file.path) && /^src\/platform\//.test(importedPath)) {
          report("shared-no-platform-import");
        }

        if (isFeatureComponentOrHook(file.path) && /^src\/platform\//.test(importedPath)) {
          report("feature-ui-no-platform-import");
        }

        if (isAppComponentOrHook(file.path) && /^src\/platform\/persistence\//.test(importedPath)) {
          report("app-shell-no-direct-persistence-import");
        }

        if (isAppComponent(file.path) && /^src\/features\//.test(importedPath)) {
          report("app-component-no-feature-import");
        }

        if (isPlatformSource(file.path) && /^src\/app\//.test(importedPath)) {
          report("platform-no-app-import");
        }

        if (isPlatformSource(file.path) && /^src\/features\//.test(importedPath)) {
          report("platform-no-feature-import");
        }

        if (isFeatureComponentOrHook(file.path) && specifier.startsWith("@tauri-apps/")) {
          report("feature-ui-no-tauri-api");
        }
      }

      if (isFeatureComponentOrHook(file.path) && ts.isCallExpression(node)) {
        const callee = node.expression;
        if ((ts.isIdentifier(callee) && callee.text === "invoke")
          || (ts.isPropertyAccessExpression(callee) && callee.name.text === "invoke")) {
          report("feature-ui-no-direct-invoke");
        }
      }

      ts.forEachChild(node, visit);
    }
    visit(source);
  }

  return violations;
}

function assertRuntimeBoundaryGuards() {
  const defaultCapability = JSON.parse(readFileSync(DEFAULT_CAPABILITY_PATH, "utf8")) as {
    windows?: string[];
    permissions?: unknown[];
  };
  const widgetCapability = JSON.parse(readFileSync(WIDGET_CAPABILITY_PATH, "utf8")) as {
    windows?: string[];
    permissions?: unknown[];
  };

  if (defaultCapability.windows?.includes("widget")) {
    throw new Error("Default capability must not include the widget window");
  }

  if (!widgetCapability.windows?.includes("widget")) {
    throw new Error("Widget capability must explicitly include the widget window");
  }

  const widgetPermissionText = JSON.stringify(widgetCapability.permissions ?? []);
  if (widgetPermissionText.includes("sql:allow-execute")) {
    throw new Error("Widget capability must not include sql:allow-execute");
  }

  const appShell = readFileSync("src/app/AppShell.tsx", "utf8");
  const widgetShell = readFileSync("src/app/widget/WidgetShell.tsx", "utf8");
  for (const [path, content] of [
    ["src/app/AppShell.tsx", appShell],
    ["src/app/widget/WidgetShell.tsx", widgetShell],
  ] as const) {
    const renderBody = content.split(/\buseEffect\s*\(/)[0] ?? content;
    if (renderBody.includes("setUiTextLanguage(")) {
      throw new Error(`${path} must not call setUiTextLanguage before its first effect`);
    }
  }

  const widgetIconService = readFileSync("src/app/widget/widgetIconService.ts", "utf8");
  if (widgetIconService.includes("platform/persistence/sessionReadRepository")) {
    throw new Error("Widget icon service must not import the session read repository directly");
  }
}

function runSelfTest() {
  const violations = findArchitectureViolations([
    {
      path: "src/features/data/components/Data.tsx",
      content: "import { getSessionsInRange } from '../../../platform/persistence/sessionReadRepository.ts';",
    },
    {
      path: "src/features/data/services/dataReadModel.ts",
      content: "const repository = await import('../../../platform/persistence/sessionReadRepository.ts');",
    },
    {
      path: "src/shared/lib/sessionReadRepository.ts",
      content: [
        "export type { HistorySession } from '../../platform/persistence/sessionReadRepository.ts';",
        "import type { View } from '../../app/types/view.ts';",
        "import Dashboard from '../../features/dashboard/components/Dashboard.tsx';",
      ].join("\n"),
    },
    {
      path: "src/features/settings/hooks/useSettings.ts",
      content: "await invoke('cmd_save_settings');",
    },
    {
      path: "src/features/settings/services/settingsRuntimeAdapterService.ts",
      content: "import { setAfkThreshold } from '../../../platform/runtime/trackingRuntimeGateway.ts';",
    },
    {
      path: "src/app/components/AppTitleBar.tsx",
      content: "import { getSessionsInRange } from '../../platform/persistence/sessionReadRepository.ts';",
    },
    {
      path: "src/app/components/AppSidebar.tsx",
      content: "import ToolsStatusChip from '../../features/tools/components/ToolsStatusChip.tsx';",
    },
    {
      path: "src/platform/persistence/sessionReadRepository.ts",
      content: "import { loadDashboardSnapshot } from '../../features/dashboard/services/dashboardReadModel.ts';",
    },
    {
      path: "src/platform/runtime/trackingRuntimeGateway.ts",
      content: "import { invoke } from '@tauri-apps/api/core';",
    },
    {
      path: "src/platform/runtime/invalid.ts",
      content: "export { boot } from '@/app/bootstrap.ts';",
    },
    {
      path: "src/features/settings/components/Invalid.tsx",
      content: "import { invoke as call } from '@tauri-apps/api/core';",
    },
  ]);

  const rules = violations.map((violation) => violation.rule).sort();
  const expectedRules = [
    "app-shell-no-direct-persistence-import",
    "app-component-no-feature-import",
    "feature-ui-no-direct-invoke",
    "feature-ui-no-platform-import",
    "feature-ui-no-tauri-api",
    "platform-no-app-import",
    "platform-no-feature-import",
    "shared-no-app-import",
    "shared-no-feature-import",
    "shared-no-platform-import",
  ].sort();

  assert.deepEqual(rules, expectedRules, "All existing boundary rules must be enforced");

  const dependencyForms = [
    "import {\n  gateway,\n} from '@/platform/runtime/gateway';",
    "import type {\r\n  Gateway,\r\n} from '../../platform/runtime/gateway';",
    "export {\n  gateway,\n} from '@/platform/runtime/gateway';",
    "export type {\n  Gateway,\n} from '@/platform/runtime/gateway';",
    "export * from '@/platform/runtime/gateway';",
    "import '@/platform/runtime/gateway';",
    "const gateway = import(\n  /* comment */ '@/platform/runtime/gateway',\n);",
    "const gateway = import(`@/platform/runtime/gateway`);",
    "type Gateway = import(\n  '@/platform/runtime/gateway'\n).Gateway;",
    "import gateway = require('@/platform/runtime/gateway');",
  ];
  for (const content of dependencyForms) {
    const found = findArchitectureViolations([{
      path: "src/shared/lib/example.ts",
      content: `// fixture header\n${content}`,
    }]);
    assert.equal(found.length, 1, content);
    assert.equal(found[0].rule, "shared-no-platform-import", content);
    assert.equal(found[0].line, 2, "Report the start of the dependency node");
  }

  assert.deepEqual(findArchitectureViolations([{
    path: "src/features/settings/components/Example.tsx",
    content: [
      "// import { invoke } from '@tauri-apps/api/core'; invoke('ignored');",
      "/* export { gateway } from '@/platform/runtime/gateway'; */",
      "const example = \"import('@/platform/runtime/gateway'); invoke('ignored')\";",
      "const label = '@tauri-apps/api/core';",
      "import { save } from '../services/settingsService';",
      "export const view = <span>invoke('example')</span>;",
    ].join("\n"),
  }]), [], "Comments, examples and service access are not boundary violations");

  assert.deepEqual(findArchitectureViolations([{
    path: "src/features/settings/hooks/useExample.ts",
    content: [
      "const api = import(\n '@tauri-apps/api/core'\n);",
      "invoke<string>(\n 'command'\n);",
      "api.invoke(\n 'command'\n);",
    ].join("\n"),
  }]).map(({ rule }) => rule), [
    "feature-ui-no-tauri-api",
    "feature-ui-no-direct-invoke",
    "feature-ui-no-direct-invoke",
  ]);
}

function main() {
  runSelfTest();
  if (process.argv.includes("--self-test")) {
    console.log("Architecture boundary self-test passed");
    return;
  }

  const files = SCAN_ROOTS.flatMap((root) => collectSourceFiles(root));
  const violations = findArchitectureViolations(files);
  assertRuntimeBoundaryGuards();

  if (violations.length === 0) {
    console.log("Architecture boundary check passed");
    return;
  }

  console.error("Architecture boundary check failed. UI, shell, and platform code must stay within owned boundaries.");
  for (const violation of violations) {
    console.error(`${violation.path}:${violation.line} ${violation.rule} -> ${violation.text}`);
  }
  process.exitCode = 1;
}

main();
