MAKEFLAGS += -sBj$(nproc)

CXX = clang++
CXXFLAGS = -std=c++17 -Wall -O3 -DNDEBUG -flto
FRAMEWORKS = -framework CoreGraphics -framework AudioToolbox -framework CoreFoundation -framework CoreAudio -framework Accelerate
TARGET = kream_engine
SRCDIR = src
BUILDDIR = build
THEMEDIR = themes/V1
SOURCES = $(wildcard $(SRCDIR)/*.cpp)
OBJECTS = $(SOURCES:$(SRCDIR)/%.cpp=$(BUILDDIR)/%.o)

all: $(BUILDDIR)/$(TARGET)

$(BUILDDIR)/$(TARGET): $(OBJECTS)
	$(CXX) $(CXXFLAGS) -o $@ $^ $(FRAMEWORKS)

$(BUILDDIR)/%.o: $(SRCDIR)/%.cpp
	mkdir -p $(BUILDDIR)
	$(CXX) $(CXXFLAGS) -c -o $@ $<

run:
	$(MAKE) clean
	$(MAKE) all
	cp -r $(THEMEDIR)/* $(BUILDDIR)/
	cd $(BUILDDIR) && ./$(TARGET)

clean:
	rm -rf $(BUILDDIR)

.PHONY: all run clean
