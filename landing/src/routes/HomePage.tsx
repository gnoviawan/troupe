import { Bento } from '../components/sections/Bento';
import { ClosingCta } from '../components/sections/ClosingCta';
import { Facts } from '../components/sections/Facts';
import { FaqSection } from '../components/sections/FaqSection';
import { Hero } from '../components/sections/Hero';
import { HeroShot } from '../components/sections/HeroShot';
import { MoreInOnePlace } from '../components/sections/MoreInOnePlace';
import { Testimonials } from '../components/sections/Testimonials';
import { WorksWith } from '../components/sections/WorksWith';
import { FeatureTour } from '../components/tour/FeatureTour';
import { TOUR_TABS } from '../data/tour';

export function HomePage() {
  return (
    <main id="main-content">
      <Hero />
      <HeroShot />
      <WorksWith />
      <FeatureTour tabs={TOUR_TABS} />
      <Bento />
      <MoreInOnePlace />
      <ClosingCta />
      <Facts />
      <Testimonials />
      <FaqSection />
    </main>
  );
}
