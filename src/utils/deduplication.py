from __future__ import annotations

import re
from difflib import SequenceMatcher
from typing import Any

from ..config_new import settings


def extract_quality(title: str) -> str | None:
    """Extract quality indicator from title."""
    title_upper = title.upper()
    quality_patterns = [
        (r'\b4K\b', '4K'),
        (r'\b2160P\b', '2160p'),
        (r'\b1080P\b', '1080p'),
        (r'\b720P\b', '720p'),
        (r'\b480P\b', '480p'),
    ]
    for pattern, quality in quality_patterns:
        if re.search(pattern, title_upper):
            return quality
    return None


def quality_rank(quality: str | None) -> int:
    """Get quality rank based on preferences."""
    if not quality:
        return 999
    try:
        return settings.preferred_quality_order.index(quality)
    except ValueError:
        return 999


def normalize_title(title: str) -> str:
    """Normalize title for comparison."""
    # Remove common noise
    title = re.sub(r'\[.*?\]', '', title)
    title = re.sub(r'\(.*?\)', '', title)
    title = re.sub(r'【.*?】', '', title)
    title = re.sub(r'\d{4}P?', '', title, flags=re.I)  # Remove resolution
    title = re.sub(r'\b4K\b', '', title, flags=re.I)
    title = re.sub(r'[^\w\s]', ' ', title)
    title = ' '.join(title.split())
    return title.lower().strip()


def title_similarity(title1: str, title2: str) -> float:
    """Calculate similarity between two titles (0-1)."""
    norm1 = normalize_title(title1)
    norm2 = normalize_title(title2)
    return SequenceMatcher(None, norm1, norm2).ratio()


def deduplicate_results(results: list[dict[str, Any]], similarity_threshold: float = 0.85) -> list[dict[str, Any]]:
    """
    Deduplicate search results based on title similarity and quality preference.
    
    For similar titles, keep the one with better quality.
    """
    if not results:
        return []
    
    # Group by normalized title
    groups: dict[str, list[dict[str, Any]]] = {}
    
    for item in results:
        title = item.get("title") or item.get("note") or ""
        if not title:
            continue
        
        # Find similar group
        found_group = False
        for group_key in groups:
            if title_similarity(title, group_key) >= similarity_threshold:
                groups[group_key].append(item)
                found_group = True
                break
        
        if not found_group:
            groups[title] = [item]
    
    # Select best from each group
    deduplicated = []
    for group_items in groups.values():
        if len(group_items) == 1:
            deduplicated.append(group_items[0])
        else:
            # Sort by quality preference
            best = min(group_items, key=lambda x: quality_rank(extract_quality(x.get("title") or x.get("note") or "")))
            deduplicated.append(best)
    
    return deduplicated


def enhance_results_with_quality(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Add quality metadata to results."""
    for item in results:
        title = item.get("title") or item.get("note") or ""
        quality = extract_quality(title)
        item["quality"] = quality
        item["quality_rank"] = quality_rank(quality)
    return results
