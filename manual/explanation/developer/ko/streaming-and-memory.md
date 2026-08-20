---
type: Explanation
title: 스트리밍 및 메모리 동작 방식
description: 단일 엔트리 리더 타입이 다양한 백엔드 메모리 프로필을 다루는 이유와 스트림 한계 초과 시 오류가 발생하는 이유를 설명합니다.
tags: [streaming, extraction, decision, DCR-006, IG-004-01]
audience: developer
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/explanation/developer/en/streaming-and-memory.md }
synced_hash: 6981e265060df5657ff66e8b0b1a69e6a7dcb41e91cf23de8da087bdae05a54b
---
# 스트리밍 및 메모리 동작 방식

## 개요

`StreamingExtractor`는 다양한 백엔드에 대해 동일한 점진적 읽기 인터페이스를 제공합니다.

백엔드별 메모리 사용량 특성 설명.

## 두 가지 메커니즘

진짜 스트리밍 vs 메모리 버퍼링 구조.

libarchive 지원 포맷: 진짜 스트리밍 방식.

- TAR, ISO, 단독 압축 스트림 등

ZIP, 7z, RAR 포맷: 메모리 버퍼링 연산 방식.

## 바운드 및 안전 조치

`StreamBound`를 통한 리소스 보호.

한계 초과 시 오류 반환 방식.

## 설계 결정 배경

통합 파사드 인터페이스 제공 목적.

DCR-006 결정 기록 참조.

메모리 최적화 가이드라인.

기타 고려사항.

추가 설명.

보안과의 연계.

자원 제한 설정과의 연계.

정리.

## 개발자 지침

백엔드 특성에 맞춘 사용 방법 권장.

- 대용량 데이터 처리 시 libarchive 포맷 권장

주의사항.

## 관련 문서

관련 설계 기록 및 가이드 문서 안내.

[대용량 엔트리 스트리밍 방법](../../../how-to/user/ko/stream-a-large-entry.md) 참조.

[옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md) 참조.

[에러 및 경고](../../../reference/user/ko/errors-and-warnings.md) 참조.

[포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md) 참조.

최종 요약.
