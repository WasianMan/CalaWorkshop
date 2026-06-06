import { axiosInstance } from '@/api/axios.ts';

export type SteamAccount = {
  label: string;
  valid: boolean;
};

export default async (): Promise<SteamAccount[]> => {
  const { data } = await axiosInstance.get(`/api/client/calaworkshop/steam/accounts`);
  return data.accounts ?? [];
};
