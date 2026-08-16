import { axiosInstance } from '@/api/axios.ts';
import { presetFromWire, type GamePreset } from '../getConfig.ts';

export type AdminSettings = {
  helperUrl: string;
  helperTokenSet: boolean;
  steamApiKeySet: boolean;
  defaultAnonymous: boolean;
  gamePresets: GamePreset[];
};

export default async (): Promise<AdminSettings> => {
  const { data } = await axiosInstance.get(`/api/admin/extensions/dev.wasian.calaworkshop/settings`);
  return {
    helperUrl: data.helper_url ?? '',
    helperTokenSet: data.helper_token_set ?? false,
    steamApiKeySet: data.steam_api_key_set ?? false,
    defaultAnonymous: data.default_anonymous ?? true,
    gamePresets: (data.game_presets ?? []).map(presetFromWire),
  };
};
