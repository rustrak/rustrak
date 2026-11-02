import { ConcealIcon, Input, InputAction, RevealIcon } from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';

/** A password input with a button that shows what was typed. */
export function PasswordInput({
  className,
  'data-slot': _formSlot,
  size: _size,
  ...props
}: Omit<React.ComponentProps<'input'>, 'type'> & { 'data-slot'?: string }) {
  const t = useTranslations('common');
  const [visible, setVisible] = useState(false);

  // `FormControl` clones its props onto this element. Its `data-slot` is
  // dropped, and `className` goes to the box, as it went to the group before.
  return (
    <Input
      type={visible ? 'text' : 'password'}
      boxClassName={className}
      action={
        <InputAction
          icon={visible ? ConcealIcon : RevealIcon}
          tabIndex={-1}
          onClick={() => setVisible((prev) => !prev)}
          aria-label={visible ? t('hidePassword') : t('showPassword')}
        />
      }
      {...props}
    />
  );
}
