import { ApiClient } from "./client";
import type { ApiTag } from "./types";

export class TagsApi {
  private client: ApiClient;

  constructor(client: ApiClient) {
    this.client = client;
  }

  async fetchTags(limit: number, offset: number): Promise<ApiTag[]> {
    return this.client.get<ApiTag[]>("/tags", {
      limit,
      offset,
    });
  }
}
