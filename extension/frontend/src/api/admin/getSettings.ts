import { axiosInstance } from '@/api/axios.ts';
import type { GamePreset } from '@/api/getConfig.ts';

export type AdminSettings = {
  helperUrl: string;
  helperTokenSet: boolean;
  steamApiKeySet: boolean;
  defaultAnonymous: boolean;
  gamePresets: GamePreset[];
};

export default async (): Promise<AdminSettings> => {
  const { data } = await axiosInstance.get(`/api/admin/extensions/dev.wasian.calaworkshop/settings`);
  return data;
};
