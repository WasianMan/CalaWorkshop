import { axiosInstance } from '@/api/axios.ts';
import type { WorkshopSearchItem } from './searchWorkshop.ts';

export type CollectionSkippedItem = {
  publishedFileId: number;
  reason: string;
};

export type CollectionPreview = {
  collection?: WorkshopSearchItem | null;
  children: WorkshopSearchItem[];
  skipped: CollectionSkippedItem[];
  cached: boolean;
};

export type CollectionInstallResult = {
  collectionId: number;
  jobs: { jobId: string; state: string }[];
  skipped: CollectionSkippedItem[];
};

export async function previewCollection(
  serverUuid: string,
  input: { appId: number; collectionId: number },
): Promise<CollectionPreview> {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/collections/preview`,
    {
      app_id: input.appId,
      collection_id: input.collectionId,
    },
  );
  return data;
}

export async function installCollection(
  serverUuid: string,
  input: { appId: number; collectionId: number; account?: string | null },
): Promise<CollectionInstallResult> {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/collections/install`,
    {
      app_id: input.appId,
      collection_id: input.collectionId,
      account: input.account ?? null,
    },
  );
  return data;
}
