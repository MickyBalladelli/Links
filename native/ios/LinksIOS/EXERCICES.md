# TODO.md — Spécifications : Moteur d'Animation 3D Multiplateforme & Gestionnaire d'Exercices

Ce document détaille les tâches nécessaires pour implémenter la création par commandes, l'édition, la persistance et la diffusion multiplateforme de séquences 3D d'exercices physiques.

---

## 1. Modèle de Données & Moteur de Séquence (Approche par Commandes)

- [ ] **Définir la structure du schéma d'exercice (JSON Schema / Codable)**
  - [ ] Définir le modèle `ExerciseConfig` (ID, titre, asset 3D du mannequin, nombre de répétitions, métadonnées).
  - [ ] Définir les objets d'instructions `AnimationStep` (nom du clip, durée cible, facteur de vitesse, type d'action : `PLAY_CLIP` ou `HOLD_POSE`).
- [ ] **Implémenter le moteur d'exécution des commandes (Command Runner Engine)**
  - [ ] Créer l'interpréteur de commandes qui parcourt le tableau `sequence` étape par étape.
  - [ ] Gérer le temps d'exécution réel calculé via `duration / speed`.
  - [ ] Gérer la transition fluide (*blend in / blend out*) au passage d'un clip au suivant.
  - [ ] Émettre des événements d'état pour l'UI (`onStepStarted`, `onStepCompleted`, `onExerciseFinished`).

---

## 2. API de Création & Opérations CRUD (ExerciseBuilder)

- [ ] **Implémenter la classe `ExerciseBuilder` (API Fluent)**
  - [ ] Développer la méthode `setModel(assetName: String)`.
  - [ ] Implémenter les commandes d'assemblage :
    - [ ] `addClip(name: String, duration: Double, speed: Float)` — Commande `ADD_CLIP`
    - [ ] `insertClip(at index: Int, name: String, ...)` — Commande `INSERT_CLIP`
    - [ ] `addHold(duration: Double)` — Commande `HOLD_POSE`
    - [ ] `removeClip(at index: Int)` — Commande `REMOVE_CLIP`
    - [ ] `setSpeed(at index: Int, speed: Float)` — Commande `SET_SPEED`
    - [ ] `reorderClips(from oldIndex: Int, to newIndex: Int)` — Commande `REORDER`
    - [ ] `setRepetitions(count: Int)`
  - [ ] Implémenter la méthode d'export `build() -> ExerciseConfig`.
- [ ] **Persistance des exercices (CRUD Storage)**
  - [ ] Implémenter `saveToLocal(config: ExerciseConfig)` pour l'écriture du JSON dans le dossier `Documents`.
  - [ ] Implémenter `loadFromLocal(id: String) -> ExerciseConfig`.
  - [ ] Implémenter la suppression physique du JSON et le nettoyage des références.

---

## 3. Interfaces Éditeur & Prévisualisation (UI/UX)

- [ ] **Créer l'interface d'édition visuelle d'un exercice**
  - [ ] Développer la timeline des blocs (liste réordonnable par Drag & Drop).
  - [ ] Ajouter les contrôles d'édition par bloc (sliders de durée et de vitesse).
- [ ] **Développer les contrôles du lecteur 3D**
  - [ ] Implémenter les boutons Play, Pause, Restart.
  - [ ] Ajouter le contrôle de la caméra 3D (rotation 360°, zoom, pan).
  - [ ] Ajouter une barre de progression reflétant l'avancement global de l'exercice.

---

## 4. Rendu Multiplateforme & Lecteurs Dédies

### A. Écosystème Apple (iOS, macOS, watchOS) — Swift / SwiftUI
- [ ] **iOS & macOS** : Implémenter la vue `SceneView` / `RealityView` écoutant l'`ExerciseAnimationManager`.
- [ ] **watchOS** : Implémenter une vue `SceneView` allégée pour Apple Watch et gérer la réception des JSON d'exercices depuis l'iPhone.

### B. Android — Kotlin / Jetpack Compose
- [ ] Intégrer le moteur **Filament** ou **SceneView for Android**.
- [ ] Créer la classe Kotlin équivalente `ExerciseBuilder` et le lecteur de séquences `.glb`/`.gltf`.

### C. Web & HTTP — JavaScript / Three.js
- [ ] **Lecteur Web** : Créer l'interpréteur JSON en TypeScript avec **Three.js** (`AnimationMixer`).
- [ ] **API HTTP** : Déployer les endpoints REST d'échange des configurations JSON et des modèles 3D (`/api/v1/exercises`).

---

## 5. Banque Exhaustive de Clips 3D Atomiques (Banque d'Animations)

Chaque clip doit être exporté sur le même squelette neutre aux formats **`.usdz`** (Apple) et **`.glb`** (Android / Web).

### A. Déplacements & Cardio Global
- [ ] **Marche :** `walk_start`, `walk_loop`, `walk_stop`
- [ ] **Course (Jogging & Sprint) :** `run_slow_loop`, `run_fast_loop`, `sprint_loop`
- [ ] **Pas latéraux :** `side_step_left`, `side_step_right`
- [ ] **Talon-fesses & Montées de genoux :** `butt_kicks_loop`, `high_knees_loop`
- [ ] **Sauts simples :** `jump_vertical`, `jump_forward`, `jump_land`
- [ ] **Sauts combinés :** `jumping_jacks_loop`, `star_jump`, `skater_jump`
- [ ] **Burpees :** `burpee_drop`, `burpee_pushup_bottom`, `burpee_jump`

### B. Haut du Corps (Bras, Épaules, Pectoraux, Dos)
- [ ] **Pompes (Push-ups) :** `pushup_start`, `pushup_down`, `pushup_hold`, `pushup_up`
- [ ] **Pompes variantes :** `pushup_knee_down`, `pushup_diamond_down`, `pushup_wide_down`
- [ ] **Élévations & Flexions de bras :**
  - [ ] `arm_raise_front` (Lever le bras devant)
  - [ ] `arm_raise_side` (Élévation latérale)
  - [ ] `arm_raise_overhead` (Lever les bras au-dessus de la tête)
  - [ ] `biceps_curl` (Flexion biceps)
  - [ ] `triceps_dip_down`, `triceps_dip_up` (Dips sur chaise/sol)
- [ ] **Mouvements d'épaules & rotation :**
  - [ ] `arm_circles_forward_loop`, `arm_circles_backward_loop` (Rotations de bras)
  - [ ] `shoulder_press_up`, `shoulder_press_down`

### C. Bas du Corps (Jambes, Fessiers, Hanches)
- [ ] **Squats :** `squat_start`, `squat_down`, `squat_hold`, `squat_up`
- [ ] **Squats variantes :** `squat_jump`, `sumo_squat_down`, `pistol_squat_left_down`, `pistol_squat_right_down`
- [ ] **Fentes (Lunges) :**
  - [ ] `lunge_forward_left_down`, `lunge_forward_left_up`
  - [ ] `lunge_forward_right_down`, `lunge_forward_right_up`
  - [ ] `lunge_reverse_left_down`, `lunge_side_left_down`
- [ ] **Extenseurs & Abducteurs de hanche :**
  - [ ] `leg_kick_back_left`, `leg_kick_back_right` (Coup de pied arrière)
  - [ ] `leg_raise_side_left`, `leg_raise_side_right` (Élévation latérale de jambe)
  - [ ] `donkey_kick_left`, `donkey_kick_right` (Au sol)
  - [ ] `glute_bridge_up`, `glute_bridge_hold`, `glute_bridge_down` (Pont fessier)

### D. Sangle Abdominale & Gainage (Core)
- [ ] **Gainage statique :** `plank_forearm_hold`, `plank_high_hold`, `side_plank_left_hold`, `side_plank_right_hold`
- [ ] **Gainage dynamique :** `mountain_climber_loop`, `plank_jack_loop`
- [ ] **Abdominaux :**
  - [ ] `crunch_up`, `crunch_hold`, `crunch_down`
  - [ ] `situp_up`, `situp_down`
  - [ ] `leg_raise_lying_up`, `leg_raise_lying_down` (Ciseau / Lever de jambes allongé)
  - [ ] `russian_twist_left`, `russian_twist_right`

### E. Mobilité, Étirements & Poses Statiques
- [ ] **Postures de repos / Attente :** `idle_standing`, `idle_kneeling`, `idle_plank`
- [ ] **Étirements & Flexions du tronc :**
  - [ ] `torso_rotation_left`, `torso_rotation_right` (Rotations du buste)
  - [ ] `standing_forward_bend` (Pencher le buste en avant / toucher les pieds)
  - [ ] `side_bend_left`, `side_bend_right` (Inclinaison latérale)
- [ ] **Mobilité articulaire :** `neck_rotation_loop`, `hip_rotation_loop`, `ankle_rotation_loop`