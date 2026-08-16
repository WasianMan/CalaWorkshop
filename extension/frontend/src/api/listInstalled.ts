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

/** Map a raw snake_case installed item from the backend to the camelCase type. */
export const installedFromWire = (i: any): InstalledEntry => ({
  id: i.id ?? null,
  title: i.title ?? '',
  appId: i.app_id ?? 0,
  workshopId: i.workshop_id != null ? Number(i.workshop_id) : null,
  installPath: i.install_path ?? '',
  vpkFile: i.vpk_file ?? null,
  imageFile: i.image_file ?? null,
  files: i.files ?? [],
  source: i.source ?? 'managed',
});

export default async (serverUuid: string): Promise<InstalledEntry[]> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/installed`,
  );
  return (data.items ?? []).map(installedFromWire);
};
