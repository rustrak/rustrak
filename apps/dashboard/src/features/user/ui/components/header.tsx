import type { User } from '@rustrak/client';
import { Link, useNavigate } from '@tanstack/react-router';
import { LogOut, Settings } from 'lucide-react';
import { type ReactNode, useTransition } from 'react';
import { useTranslations } from 'use-intl';
import { logout } from '@/features/user/api/mutations';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/shared/ui/components/shadcn/dropdown-menu';

interface HeaderProps {
  user: User;
  /**
   * Passed in rather than rendered here: the command bar spans several slices,
   * and this one only knows about `user`.
   */
  commandBar?: ReactNode;
}

export function Header({ user, commandBar }: HeaderProps) {
  const navigate = useNavigate();
  const t = useTranslations('user');
  const [isPending, startTransition] = useTransition();

  const handleLogout = () => {
    startTransition(async () => {
      await logout();
      navigate({ to: '/login' });
    });
  };

  return (
    <header className="h-16 flex items-center justify-between px-4 md:px-8 border-b border-border bg-background/80 backdrop-blur-md sticky top-0 z-50">
      <div className="flex items-center gap-4 md:gap-10">
        {/* The wordmark is the whole mark: no icon beside it, and the word is
            not typed next to itself. 18px because the artwork box is trimmed to
            the ink, so it reads a size larger than the number suggests. */}
        <Link to="/projects" className="flex items-center">
          <RustrakWordmark className="h-[18px] w-auto" />
        </Link>
      </div>

      {/* Search and account, grouped: both are things you do to the app rather
          than places in it, and the header reads better with one cluster at
          each end than with a third floating in the middle. */}
      <div className="flex items-center gap-2 md:gap-3">
        {commandBar}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                className="size-8 rounded-full p-0 bg-primary/20 hover:bg-primary/30"
                aria-label={t('header.openMenu')}
              />
            }
          >
            <span className="text-xs font-bold text-primary" aria-hidden="true">
              {user.email.charAt(0).toUpperCase()}
            </span>
            <span className="sr-only">
              {t('header.userMenuFor', { email: user.email })}
            </span>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <div className="px-2 py-1.5">
              <p className="text-sm font-medium truncate">{user.email}</p>
              {user.is_admin && (
                <p className="text-xs text-muted-foreground">
                  {t('roles.admin')}
                </p>
              )}
            </div>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              render={<Link to="/settings" className="cursor-pointer" />}
            >
              <Settings className="mr-2 size-4" />
              {t('header.settings')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              variant="destructive"
              onClick={handleLogout}
              disabled={isPending}
            >
              <LogOut className="mr-2 size-4" />
              {isPending ? t('header.signingOut') : t('header.signOut')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  );
}
