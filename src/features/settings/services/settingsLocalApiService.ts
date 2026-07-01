export interface LocalApiConfigurationInput {
  baseUrl: string;
  token: string;
}

export function buildLocalApiConfigurationText(input: LocalApiConfigurationInput): string {
  return [
    `PATINA_API_BASE=${input.baseUrl.trim()}`,
    `PATINA_API_TOKEN=${input.token.trim()}`,
  ].join("\n");
}
