import {
  ConcealIcon,
  Input,
  InputAction,
  type InputProps,
  RevealIcon,
} from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';

/** A password input with a button that shows what was typed. */
export function PasswordInput({
  className,
  ...props
}: Omit<InputProps, 'type' | 'action' | 'boxClassName'>) {
  const t = useTranslations('common');
  const [visible, setVisible] = useState(false);

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
