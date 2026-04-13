import type { TagsRepository } from "../db/tags";
import type { Tag } from "../db/types";

export class TagResolver {
  private tagsRepo: TagsRepository;

  constructor(tagsRepo: TagsRepository) {
    this.tagsRepo = tagsRepo;
  }

  /**
   * Resolves a tag identifier (ID or slug) to a numeric tag ID.
   *
   * @param idOrSlug - Tag ID (numeric string like "21") or slug (string like "crypto")
   * @returns Numeric tag ID as string
   * @throws Error if tag not found
   */
  resolve(idOrSlug: string): string {
    // Check if input is numeric (tag ID)
    if (/^\d+$/.test(idOrSlug)) {
      // Validate that tag exists
      const tag = this.tagsRepo.findById(idOrSlug);
      if (!tag) {
        throw new Error(
          `Tag ID '${idOrSlug}' not found in database. Run 'tags save' first.`
        );
      }
      return idOrSlug;
    }

    // Input is a slug - lookup by slug
    const tag = this.tagsRepo.findBySlug(idOrSlug);
    if (!tag) {
      throw new Error(
        `Tag slug '${idOrSlug}' not found in database. Run 'tags save' first or use numeric tag ID.`
      );
    }

    return tag.id;
  }

  /**
   * Resolves tag and returns full tag object with metadata.
   */
  resolveWithMetadata(idOrSlug: string): Tag {
    const tagId = this.resolve(idOrSlug);
    return this.tagsRepo.findById(tagId)!;
  }
}
