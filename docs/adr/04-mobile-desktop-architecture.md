# ADR-04: HIVE Mesh Mobile & Desktop Application Architecture

**Organization:** (r)evolve - Revolve Team LLC  
**URL:** https://revolveteam.com  
**Date:** January 2026  
**Status:** Draft  
**Depends On:** ADR-01 (Capability Validation), ADR-02 (Feature Requirements), ADR-03 (Embedded Architecture)

---

## Executive Summary

This ADR defines the architecture for HIVE Mesh mobile and desktop applications. We use **React Native with TypeScript** (bare workflow) as the cross-platform framework, targeting **iOS, Android, macOS, Windows, and Linux**. The architecture leverages existing UniFFI bindings to the hive-btle Rust core. This choice optimizes for community adoption, contributor accessibility, and long-term extensibility.

---

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Framework | React Native (bare workflow) | Maximum native module control, no Expo limitations |
| Desktop Support | Yes (macOS, Windows, Linux) | Full cross-platform coverage |
| Theme | Dark/Light with tactical dark default | Professional "tactical" aesthetic |
| Notifications | Local only (no server) | Mesh-first, no cloud dependency |
| Accessibility | Yes, from start | Inclusive design, government compliance |
| Localization | i18n from start | Global community, international users |

---

## Decision: React Native + TypeScript

### Why React Native?

| Factor | React Native | Flutter | Native (Swift/Kotlin) |
|--------|--------------|---------|----------------------|
| **Developer pool** | ~17M JS devs | ~2M Dart devs | Split iOS/Android |
| **Contributor accessibility** | Very High | Medium | Low (2 languages) |
| **Open source culture** | Very Strong | Strong | Mixed |
| **Code sharing** | iOS + Android + (Web) | iOS + Android | None |
| **Rust FFI** | Via native modules | flutter_rust_bridge | UniFFI (direct) |
| **Enterprise adoption** | High | Growing | Highest |
| **Startup/community** | Very High | High | Lower |

### The Community Argument

**Goal:** Maximize potential contributors to the HIVE Mesh ecosystem.

JavaScript/TypeScript is the largest developer community in the world. By choosing React Native:

- Web developers can contribute without learning new languages
- React patterns are universally understood
- The npm ecosystem provides massive library support
- Lower barrier to entry = more contributors

### Comparison with Similar Projects

| Project | Framework | Result |
|---------|-----------|--------|
| BitChat | Native Swift | iOS-first, Android port separate, limits contributors |
| Briar | Native Android | No iOS at all, Android-only community |
| Signal | Native (both) | Well-funded team, not community-driven |
| Discord | React Native | Massive cross-platform success |
| Coinbase | React Native | High-security app, cross-platform |

HIVE can differentiate by being **more accessible to contributors** than BitChat or Briar.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  React Native App (TypeScript)                                  │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │  UI Layer                                                  │ │
│  │  ├─ screens/        (Home, Messages, Map, Alerts, etc.)   │ │
│  │  ├─ components/     (MessageBubble, AlertCard, PeerList)  │ │
│  │  └─ navigation/     (React Navigation stack)              │ │
│  └───────────────────────────────────────────────────────────┘ │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │  State Layer                                               │ │
│  │  ├─ stores/         (Zustand stores for each domain)      │ │
│  │  ├─ hooks/          (useMesh, useMessages, useAlerts)     │ │
│  │  └─ types/          (TypeScript interfaces)               │ │
│  └───────────────────────────────────────────────────────────┘ │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │  Native Bridge Layer                                       │ │
│  │  └─ HiveMeshModule  (Turbo Module for Rust FFI)           │ │
│  └───────────────────────────────────────────────────────────┘ │
└─────────────────────────────────┬───────────────────────────────┘
                                  │
         ┌────────────────────────┴────────────────────────┐
         │                                                 │
         ▼                                                 ▼
┌─────────────────────────┐                 ┌─────────────────────────┐
│  iOS Native Module      │                 │  Android Native Module  │
│  (Swift)                │                 │  (Kotlin)               │
│  ┌───────────────────┐  │                 │  ┌───────────────────┐  │
│  │ HiveMeshBridge    │  │                 │  │ HiveMeshBridge    │  │
│  │ - Swift wrapper   │  │                 │  │ - Kotlin wrapper  │  │
│  │ - Event emitters  │  │                 │  │ - Event emitters  │  │
│  └─────────┬─────────┘  │                 │  └─────────┬─────────┘  │
│            │ UniFFI     │                 │            │ UniFFI     │
│            ▼            │                 │            ▼            │
│  ┌───────────────────┐  │                 │  ┌───────────────────┐  │
│  │ hive-apple-ffi    │  │                 │  │ hive-android-ffi  │  │
│  │ (generated)       │  │                 │  │ (generated)       │  │
│  └───────────────────┘  │                 │  └───────────────────┘  │
└─────────────────────────┘                 └─────────────────────────┘
                │                                         │
                └──────────────────┬──────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│  hive-btle (Rust)                                                   │
│  ├─ src/lib.rs           Core library                               │
│  ├─ src/crdt/            CRDT implementations                       │
│  ├─ src/sync/            Sync protocol                              │
│  ├─ src/app/             Application logic (messages, alerts, etc.) │
│  └─ src/platform/        Platform-specific BLE (apple, android)     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Repository Structure

```
hive-btle/
├── src/                          # Rust core
│   ├── lib.rs
│   ├── crdt/
│   ├── sync/
│   ├── app/                      # Shared app logic
│   │   ├── messages.rs
│   │   ├── alerts.rs
│   │   ├── markers.rs
│   │   ├── files.rs
│   │   └── peers.rs
│   └── platform/
│       ├── apple/                # iOS + macOS
│       ├── android/
│       ├── windows/              # WinRT BLE
│       └── linux/                # BlueZ
│
├── bindings/                     # FFI bindings
│   ├── uniffi.toml
│   ├── ios/                      # Generated Swift bindings
│   │   └── HiveBtle/
│   ├── android/                  # Generated Kotlin bindings
│   │   └── hive-btle/
│   └── desktop/                  # Generated bindings for desktop
│       ├── macos/                # Swift (shares with iOS)
│       ├── windows/              # C++/WinRT
│       └── linux/                # C/C++ for BlueZ
│
├── app/                          # React Native app (ALL PLATFORMS)
│   ├── package.json
│   ├── tsconfig.json
│   ├── app.json
│   ├── index.js
│   │
│   ├── src/
│   │   ├── App.tsx
│   │   │
│   │   ├── screens/              # Screen components
│   │   │   ├── HomeScreen.tsx
│   │   │   ├── MessagesScreen.tsx
│   │   │   ├── MessageDetailScreen.tsx
│   │   │   ├── MapScreen.tsx
│   │   │   ├── AlertsScreen.tsx
│   │   │   ├── PeersScreen.tsx
│   │   │   └── SettingsScreen.tsx
│   │   │
│   │   ├── components/           # Reusable components
│   │   │   ├── MessageBubble.tsx
│   │   │   ├── MessageInput.tsx
│   │   │   ├── AlertCard.tsx
│   │   │   ├── AlertBanner.tsx
│   │   │   ├── MarkerCard.tsx
│   │   │   ├── PeerListItem.tsx
│   │   │   ├── MeshStatus.tsx
│   │   │   └── EmergencyButton.tsx
│   │   │
│   │   ├── navigation/           # Navigation setup
│   │   │   ├── RootNavigator.tsx
│   │   │   ├── TabNavigator.tsx
│   │   │   └── types.ts
│   │   │
│   │   ├── stores/               # Zustand state stores
│   │   │   ├── meshStore.ts
│   │   │   ├── messageStore.ts
│   │   │   ├── alertStore.ts
│   │   │   ├── markerStore.ts
│   │   │   ├── peerStore.ts
│   │   │   ├── settingsStore.ts
│   │   │   └── themeStore.ts     # Theme (dark/light) state
│   │   │
│   │   ├── hooks/                # Custom React hooks
│   │   │   ├── useMesh.ts
│   │   │   ├── useMessages.ts
│   │   │   ├── useAlerts.ts
│   │   │   ├── useMarkers.ts
│   │   │   ├── usePeers.ts
│   │   │   ├── useLocation.ts
│   │   │   └── useTheme.ts       # Theme hook
│   │   │
│   │   ├── native/               # Native module interface
│   │   │   ├── HiveMesh.ts
│   │   │   └── types.ts
│   │   │
│   │   ├── theme/                # Theme system
│   │   │   ├── colors.ts         # Color palettes
│   │   │   ├── typography.ts     # Font styles
│   │   │   ├── spacing.ts        # Spacing scale
│   │   │   └── index.ts
│   │   │
│   │   ├── i18n/                 # Internationalization
│   │   │   ├── index.ts          # i18n setup
│   │   │   └── locales/
│   │   │       ├── en.json       # English (default)
│   │   │       ├── es.json       # Spanish
│   │   │       ├── fr.json       # French
│   │   │       ├── de.json       # German
│   │   │       ├── uk.json       # Ukrainian
│   │   │       └── ar.json       # Arabic (RTL)
│   │   │
│   │   ├── types/                # TypeScript types
│   │   │   ├── message.ts
│   │   │   ├── alert.ts
│   │   │   ├── marker.ts
│   │   │   ├── peer.ts
│   │   │   └── index.ts
│   │   │
│   │   └── utils/                # Utilities
│   │       ├── format.ts
│   │       ├── geo.ts
│   │       ├── permissions.ts
│   │       └── accessibility.ts  # A11y helpers
│   │
│   ├── ios/                      # iOS native code
│   │   ├── HiveMesh/
│   │   │   ├── HiveMeshModule.swift
│   │   │   ├── HiveMeshModule.mm
│   │   │   └── HiveMeshEventEmitter.swift
│   │   └── Podfile
│   │
│   ├── android/                  # Android native code
│   │   ├── app/
│   │   │   └── src/main/java/com/hivemesh/
│   │   │       ├── HiveMeshModule.kt
│   │   │       ├── HiveMeshPackage.kt
│   │   │       └── HiveMeshEventEmitter.kt
│   │   └── build.gradle
│   │
│   ├── macos/                    # macOS native code
│   │   ├── HiveMesh-macOS/
│   │   │   ├── HiveMeshModule.swift
│   │   │   └── HiveMeshEventEmitter.swift
│   │   └── Podfile
│   │
│   ├── windows/                  # Windows native code
│   │   ├── HiveMesh/
│   │   │   ├── HiveMeshModule.cpp
│   │   │   ├── HiveMeshModule.h
│   │   │   └── HiveMeshEventEmitter.cpp
│   │   └── HiveMesh.sln
│   │
│   └── __tests__/                # Jest tests
│       ├── components/
│       ├── stores/
│       └── i18n/
│
├── examples/
│   └── m5stack-core2/            # ESP32 firmware (per ADR-03)
│
└── docs/
    ├── adr/
    │   ├── 01-capability-validation.md
    │   ├── 02-feature-requirements.md
    │   ├── 03-embedded-architecture.md
    │   └── 04-mobile-desktop-architecture.md
    ├── api/
    └── contributing/
        └── LOCALIZATION.md       # Guide for adding new languages
```

---

## Technology Stack

### Target Platforms

| Platform | Framework | BLE Support | Notes |
|----------|-----------|-------------|-------|
| **iOS** | React Native | CoreBluetooth | Primary mobile target |
| **Android** | React Native | Android BLE | Primary mobile target |
| **macOS** | React Native macOS | CoreBluetooth | Shares iOS BLE code |
| **Windows** | React Native Windows | WinRT BLE | Native Windows BLE API |
| **Linux** | React Native + Electron | BlueZ | May require Electron wrapper |

### Core Dependencies

| Layer | Technology | Rationale |
|-------|------------|-----------|
| **Framework** | React Native 0.73+ (bare) | New Architecture, no Expo limitations |
| **Desktop** | react-native-macos, react-native-windows | Official Microsoft/community support |
| **Language** | TypeScript 5.x | Type safety, better DX |
| **State** | Zustand | Simple, performant, TypeScript-first |
| **Navigation** | React Navigation 6 | Industry standard |
| **Maps** | react-native-maps | Google/Apple maps (mobile), Mapbox (desktop) |
| **UI Components** | Custom + React Native Paper | Material Design base |
| **i18n** | react-i18next | Industry standard, ICU support |
| **Accessibility** | Built-in + custom | WCAG 2.1 AA target |
| **Testing** | Jest + React Native Testing Library | Standard tooling |

### Theme System

```typescript
// src/theme/colors.ts

export const darkTheme = {
  name: 'tactical',
  colors: {
    // Backgrounds
    background: '#0D1117',      // Near-black
    surface: '#161B22',         // Card/panel background
    surfaceElevated: '#21262D', // Modal/dropdown background
    
    // Text
    textPrimary: '#E6EDF3',     // Primary text
    textSecondary: '#8B949E',   // Secondary/muted text
    textDisabled: '#484F58',    // Disabled text
    
    // Accent - tactical green
    primary: '#238636',         // Primary actions
    primaryMuted: '#2EA043',    // Hover states
    
    // Status colors
    success: '#238636',         // Success/online
    warning: '#D29922',         // Warning/caution
    error: '#F85149',           // Error/danger
    info: '#58A6FF',            // Info/links
    
    // Alert-specific
    alertSOS: '#F85149',        // Emergency red
    alertMedical: '#F85149',    // Medical red
    alertEvac: '#D29922',       // Evacuation amber
    alertClear: '#238636',      // All clear green
    
    // Mesh status
    meshConnected: '#238636',   // Connected green
    meshDisconnected: '#8B949E', // Disconnected gray
    meshError: '#F85149',       // Error red
    
    // Borders
    border: '#30363D',          // Default border
    borderFocus: '#58A6FF',     // Focused input border
  },
};

export const lightTheme = {
  name: 'standard',
  colors: {
    background: '#FFFFFF',
    surface: '#F6F8FA',
    surfaceElevated: '#FFFFFF',
    
    textPrimary: '#24292F',
    textSecondary: '#57606A',
    textDisabled: '#8C959F',
    
    primary: '#238636',
    primaryMuted: '#2EA043',
    
    success: '#238636',
    warning: '#9A6700',
    error: '#CF222E',
    info: '#0969DA',
    
    alertSOS: '#CF222E',
    alertMedical: '#CF222E',
    alertEvac: '#9A6700',
    alertClear: '#238636',
    
    meshConnected: '#238636',
    meshDisconnected: '#57606A',
    meshError: '#CF222E',
    
    border: '#D0D7DE',
    borderFocus: '#0969DA',
  },
};
```

### Internationalization (i18n)

```typescript
// src/i18n/index.ts

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import * as Localization from 'react-native-localize';

import en from './locales/en.json';
import es from './locales/es.json';
import fr from './locales/fr.json';
import de from './locales/de.json';
import uk from './locales/uk.json';  // Ukrainian - relevant for defense
import ar from './locales/ar.json';  // Arabic - RTL support

const resources = { en, es, fr, de, uk, ar };

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: Localization.getLocales()[0].languageCode,
    fallbackLng: 'en',
    interpolation: {
      escapeValue: false,
    },
  });

export default i18n;
```

```json
// src/i18n/locales/en.json
{
  "common": {
    "back": "Back",
    "save": "Save",
    "cancel": "Cancel",
    "delete": "Delete",
    "edit": "Edit",
    "send": "Send",
    "retry": "Retry"
  },
  "tabs": {
    "home": "Home",
    "messages": "Messages",
    "map": "Map",
    "alerts": "Alerts",
    "settings": "Settings"
  },
  "mesh": {
    "status": "Mesh Status",
    "connected": "Connected",
    "disconnected": "Disconnected",
    "peers": "{{count}} peer",
    "peers_plural": "{{count}} peers",
    "directPeers": "Direct peers",
    "reachable": "Reachable"
  },
  "messages": {
    "title": "Messages",
    "newMessage": "New Message",
    "broadcast": "Broadcast",
    "typeMessage": "Type a message...",
    "sent": "Sent",
    "delivered": "Delivered",
    "read": "Read"
  },
  "alerts": {
    "title": "Alerts",
    "emergency": "Emergency",
    "sos": "SOS",
    "medical": "Medical",
    "evacuation": "Evacuation",
    "allClear": "All Clear",
    "checkIn": "Check In",
    "acknowledge": "Acknowledge",
    "acknowledged": "Acknowledged",
    "acks": "{{count}} of {{total}} acknowledged",
    "holdToSend": "Hold to send emergency alert",
    "sending": "Sending alert...",
    "confirmSend": "Send {{type}} alert?"
  },
  "markers": {
    "title": "Markers",
    "addMarker": "Add Marker",
    "hazard": "Hazard",
    "resource": "Resource",
    "medical": "Medical",
    "rallyPoint": "Rally Point",
    "information": "Information"
  },
  "settings": {
    "title": "Settings",
    "identity": "Identity",
    "callsign": "Callsign",
    "deviceId": "Device ID",
    "mesh": "Mesh",
    "autoConnect": "Auto-connect",
    "relayForOthers": "Relay for others",
    "shareLocation": "Share location",
    "notifications": "Notifications",
    "alertSounds": "Alert sounds",
    "messageSounds": "Message sounds",
    "vibration": "Vibration",
    "appearance": "Appearance",
    "theme": "Theme",
    "themeDark": "Tactical (Dark)",
    "themeLight": "Standard (Light)",
    "themeSystem": "System",
    "language": "Language",
    "storage": "Storage",
    "messageHistory": "Message history",
    "clearAllData": "Clear all data",
    "about": "About",
    "version": "Version"
  },
  "accessibility": {
    "meshStatus": "Mesh status: {{status}}",
    "peerCount": "{{count}} peers connected",
    "newMessage": "New message from {{sender}}",
    "emergencyAlert": "Emergency alert from {{sender}}",
    "sendEmergency": "Send emergency alert. Hold for 2 seconds.",
    "mapMarker": "{{type}} marker: {{title}}"
  }
}
```

### Accessibility

```typescript
// src/components/AccessibleButton.tsx

import React from 'react';
import { Pressable, Text, AccessibilityInfo } from 'react-native';
import { useTranslation } from 'react-i18next';

interface Props {
  onPress: () => void;
  label: string;
  accessibilityHint?: string;
  accessibilityRole?: 'button' | 'link' | 'alert';
  disabled?: boolean;
  children: React.ReactNode;
}

export const AccessibleButton: React.FC<Props> = ({
  onPress,
  label,
  accessibilityHint,
  accessibilityRole = 'button',
  disabled = false,
  children,
}) => {
  const handlePress = () => {
    // Announce to screen readers
    AccessibilityInfo.announceForAccessibility(label);
    onPress();
  };

  return (
    <Pressable
      onPress={handlePress}
      disabled={disabled}
      accessible={true}
      accessibilityLabel={label}
      accessibilityHint={accessibilityHint}
      accessibilityRole={accessibilityRole}
      accessibilityState={{ disabled }}
    >
      {children}
    </Pressable>
  );
};

// Emergency button with haptic and audio feedback
export const EmergencyButton: React.FC<{
  onLongPress: () => void;
}> = ({ onLongPress }) => {
  const { t } = useTranslation();
  
  return (
    <Pressable
      onLongPress={onLongPress}
      delayLongPress={2000}
      accessible={true}
      accessibilityLabel={t('alerts.sos')}
      accessibilityHint={t('accessibility.sendEmergency')}
      accessibilityRole="button"
    >
      {/* ... */}
    </Pressable>
  );
};
```

### Native Bridge

| Platform | Approach |
|----------|----------|
| **iOS** | Turbo Module (Swift) wrapping UniFFI bindings |
| **Android** | Turbo Module (Kotlin) wrapping UniFFI bindings |

### Why Turbo Modules?

React Native's New Architecture provides:
- Synchronous native calls (no bridge serialization)
- Better TypeScript codegen
- Improved performance
- Future-proof (old bridge deprecated)

---

## Screen Designs

### Navigation Structure

```
┌─────────────────────────────────────────────────────────────┐
│                      Tab Navigator                          │
├─────────────┬─────────────┬─────────────┬─────────────┬─────┤
│    Home     │  Messages   │     Map     │   Alerts    │  ⚙️  │
│   (mesh)    │   (chat)    │  (markers)  │   (SOS)     │     │
└─────────────┴─────────────┴─────────────┴─────────────┴─────┘
       │             │             │             │         │
       ▼             ▼             ▼             ▼         ▼
   Dashboard    Conversations   MapView     AlertList  Settings
                     │             │             │
                     ▼             ▼             ▼
               MessageDetail  MarkerDetail  AlertDetail
                     │
                     ▼
               MessageCompose
```

### 1. Home / Dashboard Screen

```
┌─────────────────────────────────────────┐
│ ≡  HIVE Mesh                    ⚡ 87%  │
├─────────────────────────────────────────┤
│                                         │
│  ┌─────────────────────────────────┐   │
│  │  MESH STATUS                    │   │
│  │  ────────────────────────────   │   │
│  │  ● Connected                    │   │
│  │  Direct peers: 3                │   │
│  │  Reachable: 7                   │   │
│  │  Signal: ████████░░ Strong      │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │  RECENT ACTIVITY                │   │
│  │  ────────────────────────────   │   │
│  │  💬 New message from Alpha-1    │   │
│  │  📍 Marker added: Rally Point   │   │
│  │  👤 Bravo-2 joined mesh         │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │        🆘 EMERGENCY 🆘          │   │
│  │        Hold to send alert       │   │
│  └─────────────────────────────────┘   │
│                                         │
├─────────────────────────────────────────┤
│  🏠      💬      🗺️      ⚠️      ⚙️   │
│  Home    Msgs    Map    Alerts   Set   │
└─────────────────────────────────────────┘
```

### 2. Messages Screen

```
┌─────────────────────────────────────────┐
│ ←  Messages                     ✏️ New  │
├─────────────────────────────────────────┤
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 📢 Mesh Broadcast          2m   │   │
│  │ Alpha-1: Copy that, en route    │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 👤 Alpha-1                  5m   │   │
│  │ Need status on north sector     │   │
│  │ ● 2 unread                      │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 👤 Bravo-2                 12m   │   │
│  │ All clear at checkpoint         │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 👤 Command                 30m   │   │
│  │ All units check in              │   │
│  └─────────────────────────────────┘   │
│                                         │
├─────────────────────────────────────────┤
│  🏠      💬      🗺️      ⚠️      ⚙️   │
└─────────────────────────────────────────┘
```

### 3. Message Detail / Conversation

```
┌─────────────────────────────────────────┐
│ ←  Alpha-1                      ℹ️ Info │
├─────────────────────────────────────────┤
│                                         │
│        ┌───────────────────────┐        │
│        │ Need status on north  │        │
│        │ sector                │        │
│        │              12:30 PM │        │
│        └───────────────────────┘        │
│                                         │
│  ┌───────────────────────┐              │
│  │ Checking now          │              │
│  │ 12:32 PM ✓✓           │              │
│  └───────────────────────┘              │
│                                         │
│        ┌───────────────────────┐        │
│        │ Copy, report back     │        │
│        │ when clear            │        │
│        │              12:33 PM │        │
│        └───────────────────────┘        │
│                                         │
│  ┌───────────────────────┐              │
│  │ All clear, no activity│              │
│  │ 12:45 PM ✓✓           │              │
│  └───────────────────────┘              │
│                                         │
├─────────────────────────────────────────┤
│ ┌─────────────────────────────┐  📷  🎤│
│ │ Type a message...           │  ➤     │
│ └─────────────────────────────┘        │
└─────────────────────────────────────────┘
```

### 4. Map Screen

```
┌─────────────────────────────────────────┐
│ ←  Map                         📍 + Add │
├─────────────────────────────────────────┤
│ ┌─────────────────────────────────────┐ │
│ │                                     │ │
│ │      🔴 Hazard                      │ │
│ │                                     │ │
│ │              📍 You                 │ │
│ │                    🟢 Rally         │ │
│ │   👤 Alpha-1                        │ │
│ │                                     │ │
│ │          👤 Bravo-2                 │ │
│ │                   🔵 Resource       │ │
│ │                                     │ │
│ │ [    ][    ][    ]                  │ │
│ │  -    +    📍                       │ │
│ └─────────────────────────────────────┘ │
│                                         │
│  Filter: [All ▼]                        │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 🔴 Downed power line    0.3 mi  │   │
│  │ 🟢 Rally Point Alpha    0.5 mi  │   │
│  │ 🔵 Water available      0.8 mi  │   │
│  └─────────────────────────────────┘   │
├─────────────────────────────────────────┤
│  🏠      💬      🗺️      ⚠️      ⚙️   │
└─────────────────────────────────────────┘
```

### 5. Alerts Screen

```
┌─────────────────────────────────────────┐
│ ←  Alerts                               │
├─────────────────────────────────────────┤
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ 🆘 ACTIVE ALERT                 │   │
│  │ ─────────────────────────────── │   │
│  │ From: Alpha-1                   │   │
│  │ Time: 12:34 PM (2 min ago)      │   │
│  │ Location: 33.749, -84.388       │   │
│  │                                 │   │
│  │ "Need immediate assistance"     │   │
│  │                                 │   │
│  │ ACKs: 3/7 peers                 │   │
│  │ ✓ You ✓ Bravo-2 ✓ Command       │   │
│  │                                 │   │
│  │ [  LOCATE  ]  [  ACK  ]         │   │
│  └─────────────────────────────────┘   │
│                                         │
│  HISTORY                                │
│  ┌─────────────────────────────────┐   │
│  │ ✅ All Clear - Bravo-2   1h ago │   │
│  │ ✅ Check In - Command    2h ago │   │
│  └─────────────────────────────────┘   │
│                                         │
├─────────────────────────────────────────┤
│  🏠      💬      🗺️      ⚠️      ⚙️   │
└─────────────────────────────────────────┘
```

### 6. Settings Screen

```
┌─────────────────────────────────────────┐
│ ←  Settings                             │
├─────────────────────────────────────────┤
│                                         │
│  IDENTITY                               │
│  ┌─────────────────────────────────┐   │
│  │ Callsign        [Alpha-3      ] │   │
│  │ Device ID       a1b2c3d4        │   │
│  └─────────────────────────────────┘   │
│                                         │
│  MESH                                   │
│  ┌─────────────────────────────────┐   │
│  │ Auto-connect           [ON ]    │   │
│  │ Relay for others       [ON ]    │   │
│  │ Share location         [ON ]    │   │
│  └─────────────────────────────────┘   │
│                                         │
│  NOTIFICATIONS                          │
│  ┌─────────────────────────────────┐   │
│  │ Alert sounds           [ON ]    │   │
│  │ Message sounds         [OFF]    │   │
│  │ Vibration              [ON ]    │   │
│  └─────────────────────────────────┘   │
│                                         │
│  STORAGE                                │
│  ┌─────────────────────────────────┐   │
│  │ Message history    [30 days ▼]  │   │
│  │ Clear all data     [  Clear  ]  │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ABOUT                                  │
│  ┌─────────────────────────────────┐   │
│  │ Version             1.0.0       │   │
│  │ hive-btle           0.1.0       │   │
│  └─────────────────────────────────┘   │
│                                         │
├─────────────────────────────────────────┤
│  🏠      💬      🗺️      ⚠️      ⚙️   │
└─────────────────────────────────────────┘
```

### 7. Emergency Alert Modal (Full Screen Takeover)

```
┌─────────────────────────────────────────┐
│                                         │
│  ⚠️ ⚠️ ⚠️ ⚠️ ⚠️ ⚠️ ⚠️ ⚠️ ⚠️ ⚠️ ⚠️  │
│                                         │
│           🆘 EMERGENCY 🆘               │
│                                         │
│         From: Alpha-1                   │
│         12:34:56 PM                     │
│                                         │
│    ┌─────────────────────────────┐     │
│    │                             │     │
│    │      📍 Location Map        │     │
│    │         (mini view)         │     │
│    │                             │     │
│    └─────────────────────────────┘     │
│                                         │
│    "Need immediate assistance           │
│     at north checkpoint"                │
│                                         │
│    Distance: 0.3 miles                  │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │         ACKNOWLEDGE             │   │
│  └─────────────────────────────────┘   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │          NAVIGATE               │   │
│  └─────────────────────────────────┘   │
│                                         │
│           [ Dismiss ]                   │
│                                         │
└─────────────────────────────────────────┘
```
*Screen pulses red, sound plays, vibration*

---

## Native Module Interface

### TypeScript Interface

```typescript
// src/native/HiveMesh.ts

export interface HiveMeshModule {
  // Lifecycle
  initialize(config: MeshConfig): Promise<void>;
  shutdown(): Promise<void>;
  
  // Connection
  startMesh(): Promise<void>;
  stopMesh(): Promise<void>;
  getMeshStatus(): Promise<MeshStatus>;
  
  // Messages
  sendMessage(message: OutgoingMessage): Promise<string>; // returns message ID
  getMessages(options?: MessageQuery): Promise<Message[]>;
  markMessageRead(messageId: string): Promise<void>;
  
  // Alerts
  sendAlert(alert: OutgoingAlert): Promise<string>;
  acknowledgeAlert(alertId: string): Promise<void>;
  getAlerts(options?: AlertQuery): Promise<Alert[]>;
  
  // Markers
  createMarker(marker: OutgoingMarker): Promise<string>;
  updateMarker(markerId: string, updates: MarkerUpdate): Promise<void>;
  deleteMarker(markerId: string): Promise<void>;
  getMarkers(options?: MarkerQuery): Promise<Marker[]>;
  
  // Peers
  getPeers(): Promise<Peer[]>;
  
  // Settings
  getSettings(): Promise<Settings>;
  updateSettings(settings: Partial<Settings>): Promise<void>;
}

// Event types emitted from native
export type HiveMeshEvent = 
  | { type: 'meshStatusChanged'; status: MeshStatus }
  | { type: 'messageReceived'; message: Message }
  | { type: 'alertReceived'; alert: Alert }
  | { type: 'alertAcknowledged'; alertId: string; peerId: string }
  | { type: 'markerCreated'; marker: Marker }
  | { type: 'markerUpdated'; marker: Marker }
  | { type: 'peerConnected'; peer: Peer }
  | { type: 'peerDisconnected'; peerId: string };
```

### Zustand Store Example

```typescript
// src/stores/messageStore.ts

import { create } from 'zustand';
import { HiveMesh } from '../native/HiveMesh';
import type { Message, OutgoingMessage } from '../types';

interface MessageState {
  messages: Message[];
  loading: boolean;
  error: string | null;
  
  // Actions
  loadMessages: () => Promise<void>;
  sendMessage: (message: OutgoingMessage) => Promise<void>;
  markRead: (messageId: string) => Promise<void>;
  
  // Event handlers (called from native listener)
  onMessageReceived: (message: Message) => void;
}

export const useMessageStore = create<MessageState>((set, get) => ({
  messages: [],
  loading: false,
  error: null,
  
  loadMessages: async () => {
    set({ loading: true, error: null });
    try {
      const messages = await HiveMesh.getMessages();
      set({ messages, loading: false });
    } catch (error) {
      set({ error: error.message, loading: false });
    }
  },
  
  sendMessage: async (message) => {
    try {
      const id = await HiveMesh.sendMessage(message);
      // Optimistic update
      set(state => ({
        messages: [...state.messages, { ...message, id, status: 'sending' }]
      }));
    } catch (error) {
      set({ error: error.message });
    }
  },
  
  markRead: async (messageId) => {
    await HiveMesh.markMessageRead(messageId);
    set(state => ({
      messages: state.messages.map(m => 
        m.id === messageId ? { ...m, read: true } : m
      )
    }));
  },
  
  onMessageReceived: (message) => {
    set(state => ({
      messages: [...state.messages, message]
    }));
  },
}));
```

---

## Implementation Phases

### Phase 1: Project Setup (1 week)

**Tasks:**
1. Initialize React Native project with TypeScript (bare workflow)
2. Configure New Architecture (Turbo Modules)
3. Set up navigation structure
4. Configure i18n with react-i18next
5. Implement theme system (dark/light)
6. Configure build for iOS and Android
7. Create placeholder screens with a11y annotations

**Deliverables:**
- [ ] Project builds and runs on iOS simulator
- [ ] Project builds and runs on Android emulator
- [ ] Navigation between all screens works
- [ ] i18n working with English strings
- [ ] Theme switching works
- [ ] Basic a11y labels in place

### Phase 2: Native Module Bridge (2 weeks)

**Tasks:**
1. Create iOS Turbo Module wrapping UniFFI bindings
2. Create Android Turbo Module wrapping UniFFI bindings
3. Implement TypeScript interface
4. Set up event emitter for native → JS events
5. Implement local notification triggers
6. Test basic round-trip communication

**Deliverables:**
- [ ] Can call Rust functions from TypeScript
- [ ] Can receive events from Rust in TypeScript
- [ ] Mesh connects and reports status
- [ ] Local notifications fire on alerts

### Phase 3: Core Features (3 weeks)

**Tasks:**
1. Implement Zustand stores for all domains
2. Build Home/Dashboard screen
3. Build Messages screen and conversation view
4. Build Alerts screen and emergency flow
5. Implement accessible components
6. Add all English strings to i18n

**Deliverables:**
- [ ] Messages send and receive
- [ ] Alerts send, receive, and acknowledge
- [ ] Emergency alert full-screen takeover works
- [ ] Mesh status displays correctly
- [ ] Screen reader compatible

### Phase 4: Map & Markers (2 weeks)

**Tasks:**
1. Integrate react-native-maps
2. Build Map screen with peer locations
3. Implement marker creation flow
4. Build marker detail view
5. Handle marker sync
6. Ensure map accessibility

**Deliverables:**
- [ ] Map displays with markers
- [ ] Can create and edit markers
- [ ] Peer locations show on map
- [ ] Markers sync across devices

### Phase 5: Desktop Platforms (2 weeks)

**Tasks:**
1. Add react-native-macos configuration
2. Add react-native-windows configuration
3. Create/adapt native modules for each platform
4. Test BLE on each platform
5. Adapt UI for larger screens (optional sidebar navigation)
6. Linux: evaluate Electron wrapper if needed

**Deliverables:**
- [ ] App runs on macOS
- [ ] App runs on Windows
- [ ] BLE works on desktop platforms
- [ ] UI adapts to larger screens

### Phase 6: Polish, i18n & Testing (2 weeks)

**Tasks:**
1. UI polish and animations
2. Complete translations (es, fr, de, uk, ar)
3. RTL layout support for Arabic
4. Error handling and edge cases
5. Offline behavior testing
6. Battery usage optimization
7. Cross-device testing (iOS ↔ Android ↔ macOS ↔ M5Stack)
8. Accessibility audit

**Deliverables:**
- [ ] App feels polished and responsive
- [ ] All 6 languages complete
- [ ] RTL works correctly
- [ ] All error states handled gracefully
- [ ] Works offline
- [ ] Battery usage acceptable
- [ ] Syncs correctly with M5Stack nodes
- [ ] WCAG 2.1 AA compliance verified

---

## Timeline Summary

| Phase | Duration | Platforms |
|-------|----------|-----------|
| 1. Project Setup | 1 week | iOS, Android |
| 2. Native Bridge | 2 weeks | iOS, Android |
| 3. Core Features | 3 weeks | iOS, Android |
| 4. Map & Markers | 2 weeks | iOS, Android |
| 5. Desktop | 2 weeks | macOS, Windows, Linux |
| 6. Polish & i18n | 2 weeks | All |
| **Total** | **12 weeks** | **5 platforms** |

---

## Resolved Decisions

| Question | Decision | Notes |
|----------|----------|-------|
| Expo or bare? | **Bare workflow** | Maximum native module control |
| State persistence? | **Rust layer** | Single source of truth in hive-btle |
| Push notifications? | **Local only** | No server, mesh-triggered local notifications |
| Dark mode? | **Yes, default** | "Tactical" dark theme default, light available |
| Accessibility? | **Yes, WCAG 2.1 AA** | From start, inclusive design |
| Localization? | **Yes, i18n from start** | react-i18next, 6 launch languages |
| Desktop? | **Yes, all three** | macOS, Windows, Linux |

### Notification Strategy (No Server)

Since we have no server component, notifications work as follows:

```
┌─────────────────────────────────────────────────────────────┐
│  App in Foreground                                          │
│  ├─ Alert received → Full-screen modal + sound + vibration │
│  └─ Message received → In-app banner                       │
├─────────────────────────────────────────────────────────────┤
│  App in Background (iOS/Android)                            │
│  ├─ BLE continues via background mode                       │
│  ├─ Alert received → Local notification + sound             │
│  └─ Message received → Local notification (silent)          │
├─────────────────────────────────────────────────────────────┤
│  App Killed                                                 │
│  └─ No notifications (BLE stops)                            │
│     User must reopen app to rejoin mesh                     │
└─────────────────────────────────────────────────────────────┘
```

**Platform-specific background BLE:**

| Platform | Background BLE | Notes |
|----------|----------------|-------|
| iOS | Yes (with entitlement) | `bluetooth-central` background mode |
| Android | Yes (foreground service) | Requires persistent notification |
| macOS | Yes | No restrictions |
| Windows | Limited | App must be running |
| Linux | Yes | BlueZ handles it |

---

## Success Metrics

| Category | Metric | Target |
|----------|--------|--------|
| **Quality** | App Store rating | 4.0+ |
| | Crash-free rate | 99.5%+ |
| **Performance** | Cold start time | <2 seconds |
| | Message delivery latency | <500ms (same mesh) |
| | Battery drain (active use) | <10%/hour |
| | Battery drain (background) | <2%/hour |
| **Community** | Contributor PRs (6 months) | 10+ |
| | Translation contributions | 3+ new languages |
| **Accessibility** | WCAG 2.1 level | AA compliance |
| | Screen reader usable | Yes |
| **Platforms** | Supported | iOS, Android, macOS, Windows, Linux |

---

## References

- ADR-01: hive-btle Capability Validation Plan
- ADR-02: Messaging App Feature Requirements
- ADR-03: Embedded (M5Stack) Architecture
- React Native New Architecture: https://reactnative.dev/docs/new-architecture-intro
- Turbo Modules: https://reactnative.dev/docs/turbo-modules
- UniFFI: https://mozilla.github.io/uniffi-rs/
- Zustand: https://github.com/pmndrs/zustand
- React Navigation: https://reactnavigation.org/
