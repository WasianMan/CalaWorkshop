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
    }));
  }

  await axiosInstance.put(`/api/admin/extensions/dev.wasian.calaworkshop/settings`, body);
};
