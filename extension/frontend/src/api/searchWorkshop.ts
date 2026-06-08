import { axiosInstance } from '@/api/axios.ts';

export type WorkshopSearchItem = {
  publishedFileId: number;
  title: string;
  previewUrl?: string | null;
  shortDescription?: string | null;
  fileSize?: number | null;
  subscriptions?: number | null;
  timeCreated?: number | null;
  timeUpdated?: number | null;
  voteScore?: number | null;
  voteCount?: number | null;
  stars?: number | null;
  fileType?: number | null;
  tags: string[];
};

export type WorkshopSearchSort = 'relevance' | 'popular' | 'trending' | 'newest' | 'updated' | 'subscribed';
export type WorkshopFileType = 'item' | 'collection';

export type WorkshopSearchResponse = {
  items: WorkshopSearchItem[];
  nextCursor?: string | null;
  total?: number | null;
  cached: boolean;
};

export default async (
  serverUuid: string,
  input: {
    appId: number;
    query?: string;
    sort?: WorkshopSearchSort;
    cursor?: string | null;
    fileType?: WorkshopFileType;
    tags?: string[];
    perPage?: number;
  },
): Promise<WorkshopSearchResponse> => {
  const { data } = await axiosInstance.get(`/api/client/servers/${serverUuid}/calaworkshop/search`, {
    params: {
      app_id: input.appId,
      q: input.query ?? '',
      sort: input.sort ?? 'popular',
      cursor: input.cursor ?? undefined,
      file_type: input.fileType ?? 'item',
      tags: input.tags && input.tags.length > 0 ? input.tags.join(',') : undefined,
      per_page: input.perPage ?? 15,
    },
  });
  return data;
};
