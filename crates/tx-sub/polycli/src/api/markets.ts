import { ApiClient } from "./client";
import type { ApiEvent } from "./types";

export class MarketsApi {
  private client: ApiClient;

  constructor(client: ApiClient) {
    this.client = client;
  }

  async fetchEvents(
    tagId: string,
    limit: number = 100,
    offset: number = 0,
    closed: boolean = false
  ): Promise<ApiEvent[]> {
    return this.client.get<ApiEvent[]>("/events", {
      tag_id: tagId,
      limit,
      offset,
      closed: closed ? "true" : "false",
    });
  }
}
