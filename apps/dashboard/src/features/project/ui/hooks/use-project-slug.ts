import { useState } from 'react';
import { slugifyPreview } from '@/features/project/model/fields';

export interface ProjectSlug {
  /** Whether the slug still follows the name, or the user took it over. */
  isManual: boolean;
  /** Re-derive from the name, unless the user has taken the field over. */
  syncFromName: (name: string) => void;
  /** The pencil / reset control: hand the field over, or give it back. */
  toggle: () => void;
  /** Hand the field over without the user asking, after a server rejection. */
  takeOver: () => void;
}

/** The two calls this hook makes on the create-project form. */
interface SlugForm {
  getFieldValue(name: 'name'): string;
  setFieldValue(name: 'slug', value: string): void;
}

/**
 * The slug, and whether it is still following the name.
 *
 * A hook rather than state inside the field because three separate places need
 * the answer: the input renders read-only while it is automatic, the submit
 * only sends a slug the user chose (a derived one is the server's to
 * de-duplicate silently, and sending the preview would turn that into a 409),
 * and a server rejection has to hand the field over or the user is left with
 * a red input they cannot edit.
 */
export function useProjectSlug(form: SlugForm): ProjectSlug {
  const [isManual, setIsManual] = useState(false);

  return {
    isManual,

    syncFromName: (name) => {
      if (isManual) return;
      form.setFieldValue('slug', slugifyPreview(name));
    },

    toggle: () => {
      if (!isManual) {
        setIsManual(true);
        return;
      }
      // Back to auto: re-derive immediately, otherwise the field would keep a
      // stale hand-typed value until the next keystroke in the name.
      setIsManual(false);
      form.setFieldValue('slug', slugifyPreview(form.getFieldValue('name')));
    },

    takeOver: () => setIsManual(true),
  };
}
