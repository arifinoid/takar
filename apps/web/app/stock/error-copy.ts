import { ApiError } from "@takar/api-client";

import type { Dictionary } from "@/lib/i18n";

/** Maps an API error code to copy the user can act on. */
export function stockError(t: Dictionary, cause: unknown): string {
  if (!(cause instanceof ApiError)) {
    return t.stock.errors.generic;
  }

  switch (cause.code) {
    case "invalid_quantity":
      return t.stock.errors.invalidQuantity;
    case "invalid_cost":
      return t.stock.errors.invalidCost;
    case "invalid_supplier":
      return t.stock.errors.invalidSupplier;
    case "material_not_found":
      return t.stock.errors.materialNotFound;
    case "name_required":
      return t.stock.errors.nameRequired;
    case "material_exists":
      return t.stock.errors.materialExists;
    case "invalid_min_stock":
      return t.stock.errors.invalidMinStock;
    case "invalid_shelf_life":
      return t.stock.errors.invalidShelfLife;
    default:
      return t.stock.errors.generic;
  }
}
