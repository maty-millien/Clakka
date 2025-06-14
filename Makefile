# --- Variables de Configuration ---

# Le compilateur C++ à utiliser. clang++ est le standard sur macOS.
CXX = clang++

# Les flags de compilation :
# -std=c++17 : Utiliser le standard C++17.
# -g         : Inclure les informations de débogage.
# -Wall      : Afficher tous les avertissements (Warning ALL).
# -O2        : Optimiser le code pour la vitesse.
CXXFLAGS = -std=c++17 -g -Wall -O2

# Les frameworks macOS requis. C'est la partie la plus importante pour la liaison.
FRAMEWORKS = -framework CoreGraphics -framework AudioToolbox -framework CoreFoundation

# Le nom de votre exécutable final.
TARGET = kream_engine

# Les répertoires de votre projet.
SRCDIR = src
BUILDDIR = build
RESDIR = res

# Trouve automatiquement tous les fichiers .cpp dans le répertoire src.
# Si vous ajoutez un nouveau fichier .cpp dans src/, vous n'aurez pas à modifier ce Makefile !
SOURCES = $(wildcard $(SRCDIR)/*.cpp)

# Génère la liste des fichiers objets (.o) qui seront placés dans le dossier build.
OBJECTS = $(SOURCES:$(SRCDIR)/%.cpp=$(BUILDDIR)/%.o)


# --- Règles de Compilation ---

# La règle par défaut, exécutée quand on tape juste "make".
# Elle dépend de l'exécutable final.
all: $(BUILDDIR)/$(TARGET)

# La règle pour créer l'exécutable final.
# Dépendances : tous les fichiers objets (.o).
# Commande : lie tous les objets ensemble en utilisant les frameworks.
$(BUILDDIR)/$(TARGET): $(OBJECTS)
	@echo "Liaison de l'exécutable..."
	$(CXX) $(CXXFLAGS) -o $@ $^ $(FRAMEWORKS)
	@echo "✅  Build terminé : $(BUILDDIR)/$(TARGET)"

# Règle générique pour compiler un fichier .cpp en fichier .o.
# Pour chaque fichier .cpp, cette règle sera utilisée.
$(BUILDDIR)/%.o: $(SRCDIR)/%.cpp
	# Crée le dossier 'build' s'il n'existe pas.
	@mkdir -p $(BUILDDIR)
	@echo "Compilation de $<..."
	$(CXX) $(CXXFLAGS) -c -o $@ $<

# --- Règles Utilitaires ---

# La règle magique pour compiler ET lancer en une seule commande !
# Dépend de 'all', donc 'make' sera exécuté d'abord pour s'assurer que tout est à jour.
run: all
	@echo "🚀  Lancement de Kream..."
	@cp -r $(RESDIR)/* $(BUILDDIR)/
	@cd $(BUILDDIR) && ./$(TARGET)

# Une règle pour nettoyer le projet (supprimer les fichiers compilés).
clean:
	@echo "Nettoyage du projet..."
	rm -rf $(BUILDDIR)

# Indique à 'make' que 'all', 'run', et 'clean' ne sont pas des noms de fichiers.
.PHONY: all run clean
