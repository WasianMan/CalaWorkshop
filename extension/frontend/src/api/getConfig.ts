import { axiosInstance } from '@/api/axios.ts';

export type GamePreset = {
  appId: number;
  name: string;
  installPath: string;
};

export type WorkshopConfig = {
  presets: GamePreset[];
  defaultAnonymous: boolean;
  helperConfigured: boolean;
  steamSearchAvailable: boolean;
  canConfigure: boolean;
  canLinkSteam: boolean;
};

export default async (serverUuid: string): Promise<WorkshopConfig> => {
  const { data } = await axiosInstance.get(`/api/client/servers/${serverUuid}/calaworkshop/config`);
  return data;
};
