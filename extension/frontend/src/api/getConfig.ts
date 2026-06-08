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

export default async (serverUuid: string): Promise<WorkshopConfig> => {
  const { data } = await axiosInstance.get(`/api/client/servers/${serverUuid}/calaworkshop/config`);
  return data;
};
