/** One page of a list whose count stops at a cap (spans, transactions). */
interface CappedPage<T> {
  items: T[];
  total_pages: number;
  total_capped: boolean;
  per_page: number;
}

/**
 * Every row of a capped list. The pages the count names are read together;
 * past a capped count it names no more, so the rest are read one after
 * another until a page comes back short.
 */
export async function allPages<T>(
  page: (n: number) => Promise<CappedPage<T>>,
): Promise<T[]> {
  const first = await page(1);
  const pages = [
    first,
    ...(await Promise.all(
      Array.from({ length: Math.max(first.total_pages - 1, 0) }, (_, i) =>
        page(i + 2),
      ),
    )),
  ];
  if (first.total_capped) {
    while (pages[pages.length - 1].items.length === first.per_page) {
      pages.push(await page(pages.length + 1));
    }
  }
  return pages.flatMap((p) => p.items);
}
