import { MotionConfig } from 'motion/react';
import type { ReactNode } from 'react';
import { Footer, Nav } from '../home/chrome';
import { SmoothScroll } from '../landing/smooth-scroll';

/**
 * The frame around the roadmap, the changelog and the blog: the landing's nav
 * and footer on the product's canvas, and nothing of the docs theme.
 */
export function Shell({ children }: { children: ReactNode }) {
  return (
    <SmoothScroll>
      {/* Under reduced motion, Motion drops every transform below (the rises,
          the masks, the drawn rules) and keeps only the fades. */}
      <MotionConfig reducedMotion="user">
        {/* `dark`: the Nextra pieces inside a post (code blocks, tables) take
            their dark variant from an ancestor class. These pages are dark-locked
            like the landing. */}
        <div className="dark relative min-h-screen bg-canvas text-fg">
          <Nav />
          {/* Opts these pages into the docs search, which indexes only marked
              regions; the docs theme used to mark them. */}
          <main data-pagefind-body>{children}</main>
          <Footer />
        </div>
      </MotionConfig>
    </SmoothScroll>
  );
}
