MAKEFLAGS += -sBj$(nproc)

CXX = clang++
CXXFLAGS = -std=c++17 -Wall -O3 -DNDEBUG -flto -Isrc -Ilib
FRAMEWORKS = -framework CoreGraphics -framework AudioToolbox -framework CoreFoundation -framework CoreAudio -framework Accelerate
TARGET = kream_engine
BUILDDIR = build
THEMEDIR = assets/themes/Creamy
SOURCES = $(shell find src -type f -name '*.cpp')
OBJECTS = $(SOURCES:src/%.cpp=$(BUILDDIR)/%.o)

all: $(BUILDDIR)/$(TARGET)

$(BUILDDIR)/$(TARGET): $(OBJECTS)
	$(CXX) $(CXXFLAGS) -o $@ $^ $(FRAMEWORKS)

$(BUILDDIR)/%.o: src/%.cpp
	mkdir -p $(dir $@)
	$(CXX) $(CXXFLAGS) -c -o $@ $<

run:
	$(MAKE) clean
	$(MAKE) all
	mkdir -p $(BUILDDIR)/assets/themes
	cp -r $(THEMEDIR) $(BUILDDIR)/assets/themes/
	cd $(BUILDDIR) && ./$(TARGET)

clean:
	rm -rf $(BUILDDIR)

.PHONY: all run clean
