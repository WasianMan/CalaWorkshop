import { axiosInstance } from '@/api/axios.ts';
import type { InstalledEntry } from './listInstalled.ts';

export default async (serverUuid: string, item: InstalledEntry): Promise<InstalledEntry> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/installed/import`,
    {
      app_id: item.appId,
      workshop_id: item.workshopId,
      title: item.title,
      install_path: item.installPath,
      files: item.files,
    },
  );
  return data;
};
