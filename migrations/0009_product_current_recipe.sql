-- Which recipe version a product is produced with.
--
-- The pointer follows the newest version: creating a recipe moves it, and an
-- old version stays readable for the production orders that consumed it.
-- A null pointer means the product cannot be produced yet.

-- Needed as the target of the composite foreign key below.
alter table recipes
    add constraint recipes_id_product_unique unique (id, product_id);

alter table products
    add column current_recipe_id uuid,
    add constraint products_current_recipe
        foreign key (current_recipe_id, id)
        references recipes (id, product_id)
        -- Column list: only the pointer is cleared, the product id stays.
        on delete set null (current_recipe_id);

-- Existing rows: point at whatever version is active.
update products p
set current_recipe_id = r.id
from recipes r
where r.product_id = p.id
  and r.is_active;
