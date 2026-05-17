"""
Unit tests for pagination.
"""
import pytest

from parvu.core.pagination import Paginator
from parvu.core.exceptions import PaginationError


def test_pagination_basic():
    paginator = Paginator(total_rows=1000, page_size=100)
    assert paginator.total_rows == 1000
    assert paginator.total_pages == 10
    assert paginator.page_size == 100


def test_pagination_offset():
    paginator = Paginator(total_rows=1000, page_size=100)
    assert paginator.page_offset(1) == 0
    assert paginator.page_offset(2) == 100
    assert paginator.page_offset(10) == 900


def test_pagination_clamp():
    paginator = Paginator(total_rows=1000, page_size=100)
    assert paginator.clamp_page(0) == 1
    assert paginator.clamp_page(1) == 1
    assert paginator.clamp_page(5) == 5
    assert paginator.clamp_page(10) == 10
    assert paginator.clamp_page(100) == 10


def test_pagination_recalculate():
    paginator = Paginator(total_rows=1000, page_size=100)
    paginator.recalculate(total_rows=500)
    assert paginator.total_rows == 500
    assert paginator.total_pages == 5


def test_pagination_invalid_page_size():
    with pytest.raises(ValueError):
        Paginator(total_rows=100, page_size=0)
