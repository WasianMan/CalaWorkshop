import { axiosInstance } from '@/api/axios.ts';
import type { GamePreset } from '../getConfig.ts';

export type UpdateSettingsInput = {
  helperUrl?: string;
  // Provide to set/clear; omit to leave unchanged.
  helperToken?: string;
  steamApiKey?: string;
  defaultAnonymous?: boolean;
  gamePresets?: GamePreset[];
};

export default async (input: UpdateSettingsInput): Promise<void> => {
  const body: Record<string, unknown> = {};
  if (input.helperUrl !== undefined) body.helper_url = input.helperUrl;
  if (input.helperToken !== undefined) body.helper_token = input.helperToken;
  if (input.steamApiKey !== undefined) body.steam_api_key = input.steamApiKey;
  if (input.defaultAnonymous !== undefined) body.default_anonymous = input.defaultAnonymous;
  if (input.gamePresets !== undefined) {
    body.game_presets = input.gamePresets.map((p) => ({
      app_id: p.appId,
      name: p.name,
      install_path: p.installPath,
      auth: p.auth ?? 'default',
      match: (p.match ?? []).map((m) => ({
        glob: m.glob,
        ...(m.rename ? { rename: m.rename } : {}),
      })),
      generated_files: (p.generatedFiles ?? []).map((g) => ({
        path: g.path,
        content: g.content,
      })),
      scan: (p.scan ?? []).map((s) => ({
        path: s.path,
        extensions: s.extensions ?? [],
        ...(s.glob ? { glob: s.glob } : {}),
      })),
      post_install: p.postInstall ?? 'none',
    }));
  }

  await axiosInstance.put(`/api/admin/extensions/dev.wasian.calaworkshop/settings`, body);
};
