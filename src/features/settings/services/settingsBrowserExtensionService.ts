type BrowserExtensionConfigInput = {
  port: number;
  token: string;
};

export function buildBrowserExtensionConfigText({
  port,
  token,
}: BrowserExtensionConfigInput) {
  return [
    "Patina Web Activity",
    `Port: ${port}`,
    `Token: ${token.trim()}`,
  ].join("\n");
}
