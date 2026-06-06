import { axiosInstance } from '@/api/axios.ts';

export type InstalledEntry = {
  name: string;
  directory?: boolean;
  size?: number;
  // Wings DirectoryEntry carries more fields; we only rely on name/size here.
  [key: string]: unknown;
};

export default async (serverUuid: string, path: string): Promise<InstalledEntry[]> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/installed`,
    { params: { path } },
  );
  return data.entries ?? [];
};
