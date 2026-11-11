import { useQuery } from '@tanstack/react-query';
import { updateCheckQuery } from '@/shared/api/version-check';
import { UpdateBanner } from '@/shared/ui/components/update-banner';

/**
 * Decides whether the update banner is shown at all.
 *
 * Exactly one of the four outcomes renders anything. `up-to-date` and
 * `disabled` render nothing because there is nothing to say; `unknown` renders
 * nothing because a banner is a claim about the running version, and this is
 * the branch where that version is precisely what could not be established.
 * Silence is the only honest output there, and it is also the only output the
 * type system cannot check, which is why it has its own test.
 *
 * The check runs after paint: it makes two requests (the server's version
 * and the public release feed) and neither may hold up the page. Arriving
 * late shifts nothing, because the banner is fixed-positioned.
 *
 * Once per tab: the query never goes stale.
 */
export function UpdateBannerSlot() {
  const { data: check } = useQuery(updateCheckQuery());

  return check?.state === 'update-available' ? (
    <UpdateBanner info={check.info} />
  ) : null;
}
