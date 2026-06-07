import { axiosInstance } from '@/api/axios.ts';

export type InstalledEntry = {
  id: string | null;
  title: string;
  appId: number;
  workshopId: number | null;
  installPath: string;
  vpkFile: string | null;
  imageFile: string | null;
  files: string[];
  source: 'managed' | 'unmanaged' | 'imported' | string;
};

export default async (serverUuid: string): Promise<InstalledEntry[]> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/installed`,
  );
  return data.items ?? [];
};
