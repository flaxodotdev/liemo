// The supplied logo, parsed into parts for LogoMark, AssembledMark and Lockup (written by scripts/brand.py
// from brand/logo/logo-mark.svg): cinetic:brand-supplied. Re-run brand.py to change it, never edit the paths.
import { type Logo, logoParts } from './Mark';

export const LOGO_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="296 163 161 237" fill="#ffffff"><path id="shard-a" d="M371 163 400 242 325 400 296 321Z"/><path id="shard-b" d="M325 400 428 321 457 400Z"/></svg>`;
export const LOGO = logoParts(LOGO_SVG, { split: 'leaves' });
export const LOGO_PARTS = ["shard-a", "shard-b"];
export const WORDMARK_SVG: string | null = null;
export const WORDMARK: Logo | null = null;
/** A raster logo is placed whole; none here: the logo is vector. */
export const LOGO_RASTER: string | null = null;
