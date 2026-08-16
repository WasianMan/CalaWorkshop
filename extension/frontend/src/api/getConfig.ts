import { axiosInstance } from '@/api/axios.ts';

export type AuthRequirement = 'default' | 'anonymous' | 'account';
export type PostInstall = 'none' | 'extract';

export type MatchRule = {
  glob: string;
  rename?: string;
};

export type GeneratedFileRule = {
  path: string;
  content: string;
};

export type ExtractFileRule = {
  format: string;
  glob: string;
  to: string;
};

export type ScanRule = {
  path: string;
  extensions?: string[];
  glob?: string;
};

export type GamePreset = {
  appId: number;
  name: string;
  installPath: string;
  // Advanced rule fields (optional; absent = mirror every file, no post-install).
  auth?: AuthRequirement;
  match?: MatchRule[];
  generatedFiles?: GeneratedFileRule[];
  extractFiles?: ExtractFileRule[];
  scan?: ScanRule[];
  postInstall?: PostInstall;
};

export type DetectionConfidence = 'high' | 'medium' | 'low';

export type WorkshopConfig = {
  presets: GamePreset[];
  defaultAnonymous: boolean;
  helperConfigured: boolean;
  steamSearchAvailable: boolean;
  canConfigure: boolean;
  canLinkSteam: boolean;
  // Best-effort app id detected from the server's egg, for preselecting a preset.
  detectedAppId?: number | null;
  detectedAppIdConfidence?: DetectionConfidence | null;
};

/**
 * The panel stopped camelCasing response bodies in 1.1.x, so responses arrive
 * with the backend's raw snake_case keys. Wire→TS mapping happens here in the
 * API layer; page components only ever see the camelCase types.
 */
export const presetFromWire = (p: any): GamePreset => ({
  appId: p.app_id ?? 0,
  name: p.name ?? '',
  installPath: p.install_path ?? '',
  auth: p.auth ?? 'default',
  match: p.match ?? [],
  generatedFiles: p.generated_files ?? [],
  extractFiles: p.extract_files ?? [],
  scan: p.scan ?? [],
  postInstall: p.post_install ?? 'none',
});

export default async (serverUuid: string): Promise<WorkshopConfig> => {
  const { data } = await axiosInstance.get(`/api/client/servers/${serverUuid}/calaworkshop/config`);
  return {
    presets: (data.presets ?? []).map(presetFromWire),
    defaultAnonymous: data.default_anonymous ?? true,
    helperConfigured: data.helper_configured ?? false,
    steamSearchAvailable: data.steam_search_available ?? false,
    canConfigure: data.can_configure ?? false,
    canLinkSteam: data.can_link_steam ?? false,
    detectedAppId: data.detected_app_id ?? null,
    detectedAppIdConfidence: data.detected_app_id_confidence ?? null,
  };
};
